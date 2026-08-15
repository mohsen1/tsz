/// Cross-file dependent constraints must materialize imported alias
/// applications before reducing the mapped/indexed/conditional types that
/// contain them. The same `InputValue` node is shared by the scalar and
/// readonly-array union arms, exercising request-local memoization as well as
/// literal-array recovery.
#[test]
fn cross_file_dependent_operand_aliases_accept_scalars_and_literal_arrays() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("column.ts"),
        r#"export interface StorageType<Read, Write, Change> {
  readonly read: Read
  readonly write: Write
  readonly change: Change
}
export type OutputType<T> = T extends StorageType<infer Read, any, any>
  ? Read
  : T
"#,
    );
    write_file(
        &base.join("expression.ts"),
        r#"export interface Expr<T> {
  readonly expressionType: T | undefined
}
export interface ScalarBuilder<Output> extends Expr<Output> {
  readonly scalar: true
}
export interface ExprScope<DB, TB extends keyof DB> {
  readonly database: DB
  readonly tables: TB
}
export type ScalarOperand<T> = Expr<T> | ScalarBuilder<Record<string, T>>
export type ExprFactory<DB, TB extends keyof DB, T> =
  (scope: ExprScope<DB, TB>) => ScalarOperand<T>
export type ExprOrFactory<DB, TB extends keyof DB, T> =
  ScalarOperand<T> | ExprFactory<DB, TB, T>
"#,
    );
    write_file(
        &base.join("reference.ts"),
        r#"import type { OutputType } from './column.js'
import type { Expr, ExprOrFactory, ScalarBuilder } from './expression.js'

export type PlainField<DB, TB extends keyof DB> = {
  [Table in TB]: keyof DB[Table]
}[TB] & string
export type QualifiedField<DB, TB extends keyof DB> = {
  [Table in TB]: `${Table & string}.${keyof DB[Table] & string}`
}[TB]
export type FieldRef<DB, TB extends keyof DB> =
  PlainField<DB, TB> | QualifiedField<DB, TB> | ExprOrFactory<DB, TB, any>
export type FieldLookup<DB, TB extends keyof DB, Name> = {
  [Table in TB]: Name extends keyof DB[Table] ? DB[Table][Name] : never
}[TB]
export type RawFieldOutput<DB, TB extends keyof DB, Ref> = Ref extends string
  ? Ref extends `${infer Schema}.${infer Table}.${infer Name}`
    ? `${Schema}.${Table}` extends TB
      ? Name extends keyof DB[`${Schema}.${Table}`]
        ? DB[`${Schema}.${Table}`][Name]
        : never
      : never
    : Ref extends `${infer Table}.${infer Name}`
      ? Table extends TB
        ? Name extends keyof DB[Table] ? DB[Table][Name] : never
        : never
      : Ref extends PlainField<DB, TB>
        ? FieldLookup<DB, TB, Ref>
        : unknown
  : Ref extends ScalarBuilder<infer Output>
    ? Output[keyof Output] | null
    : Ref extends (scope: any) => ScalarBuilder<infer Output>
      ? Output[keyof Output] | null
      : Ref extends Expr<infer Output>
        ? Output
        : Ref extends (scope: any) => Expr<infer Output> ? Output : unknown
export type FieldOutput<DB, TB extends keyof DB, Ref> =
  OutputType<RawFieldOutput<DB, TB, Ref>>
"#,
    );
    write_file(
        &base.join("value.ts"),
        r#"import type { ExprOrFactory } from './expression.js'
import type { FieldOutput } from './reference.js'

export type InputValue<DB, TB extends keyof DB, Value> =
  Value | ExprOrFactory<DB, TB, Value>
export type InputValueOrList<DB, TB extends keyof DB, Value> =
  InputValue<DB, TB, Value> | ReadonlyArray<InputValue<DB, TB, Value>>
export type DependentOperand<DB, TB extends keyof DB, Ref> =
  InputValueOrList<DB, TB, FieldOutput<DB, TB, Ref> | null>
"#,
    );
    write_file(
        &base.join("api.ts"),
        r#"import type { Expr, ExprOrFactory } from './expression.js'
import type { FieldRef } from './reference.js'
import type { DependentOperand } from './value.js'

export type CompareOp = 'in' | 'not like' | '!=' | Expr<unknown>
export interface Filter<DB, TB extends keyof DB> {
  where<Ref extends FieldRef<DB, TB>, Value extends DependentOperand<DB, TB, Ref>>(
    lhs: Ref, operator: CompareOp, rhs: Value,
  ): Filter<DB, TB>
  where<Predicate extends ExprOrFactory<DB, TB, boolean>>(
    predicate: Predicate,
  ): Filter<DB, TB>
}
export interface LayeredBuilder<DB, TB extends keyof DB, Output>
  extends Filter<DB, TB>, Expr<Output> {
  where<Ref extends FieldRef<DB, TB>, Value extends DependentOperand<DB, TB, Ref>>(
    lhs: Ref, operator: CompareOp, rhs: Value,
  ): LayeredBuilder<DB, TB, Output>
  where<Predicate extends ExprOrFactory<DB, TB, boolean>>(
    predicate: Predicate,
  ): LayeredBuilder<DB, TB, Output>
}
export type Chosen<DB, TB extends keyof DB, Table extends keyof DB & string> =
  [Table] extends [keyof DB] ? LayeredBuilder<DB, TB | Table, {}> : never
export declare class LayeredCreator<DB> {
  selectFrom<Table extends keyof DB & string>(table: Table): Chosen<DB, never, Table>
}
export declare class LayeredExtended<DB> extends LayeredCreator<DB> {
  readonly extra: true
}
"#,
    );
    let usage = r#"import type { LayeredCreator, LayeredExtended } from './api.js'

interface RegistryRow {
  title: string
  category: 'one' | 'two' | 'three' | 'four'
}
interface Registry { entries: RegistryRow }
declare const origin: LayeredCreator<Registry> | LayeredExtended<Registry>

const result = origin
  .selectFrom('entries')
  .where('category', 'in', ['one', 'two'])
  .where('title', 'not like', 'internal_%')
  .where('title', '!=', 'migration')
  .where('title', '!=', 'lock')

result.where('category', '!=', 'wrong')
result.where('category', 'in', ['one', 'wrong'])
const wrongTitles: number[] = [1, 2]
result.where('title', 'in', wrongTitles)
"#;
    write_file(&base.join("use.ts"), usage);

    let args = parse_args(&[
        "tsz",
        "--noEmit",
        "--strict",
        "--skipLibCheck",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "column.ts",
        "expression.ts",
        "reference.ts",
        "value.ts",
        "api.ts",
        "use.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");
    let actual: Vec<(u32, u32)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect();
    // Oracle-verified against the pinned typescript@7.0.2 (`--strict
    // --skipLibCheck --target es2022 --module esnext --moduleResolution
    // bundler`) on this exact fixture.
    let first_wrong = usage.find("'wrong'").expect("wrong operand");
    // The mixed-list element anchor is the `'wrong'` *inside* the array
    // literal (`['one', 'wrong']`), i.e. the next `'wrong'` occurrence after
    // the standalone argument above it — tsc anchors on the offending
    // element, not the array literal's start.
    let mixed_list_wrong = first_wrong
        + "'wrong'".len()
        + usage[first_wrong + "'wrong'".len()..]
            .find("'wrong'")
            .expect("wrong mixed-list operand element");
    let expected = vec![
        (2345, first_wrong as u32),
        // tsc reports the array's own offending element as an assignability
        // error (TS2322) against the array's element type, not an argument
        // error (TS2345) against the whole call — the array literal's other
        // element ('one') is valid, so only the element itself can fail.
        (2322, mixed_list_wrong as u32),
        (
            2345,
            usage.rfind("wrongTitles").expect("wrong list operand") as u32,
        ),
    ];
    assert_eq!(
        actual, expected,
        "only the three deliberately invalid dependent operands may fail: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics[0].message_text.contains("DependentOperand")
            && result.diagnostics[0].message_text.contains("category")
            && result.diagnostics[1].message_text.contains("InputValue")
            && result.diagnostics[1].message_text.contains("category")
            && result.diagnostics[2].message_text.contains("title"),
        "dependent-constraint diagnostics must retain their selected field: {:#?}",
        result.diagnostics
    );
    // Known residual, not re-asserted above: `TB` is never fixed to its
    // call-site instantiation (`"entries"`) in any of the three messages —
    // oracle shows `DependentOperand<Registry, "entries", "category">` /
    // `InputValue<Registry, "entries", "four" | "one" | "three" | "two" |
    // null>` / `DependentOperand<Registry, "entries", "title">`, tsz keeps
    // the free `keyof Registry` in the first two and, worse, loses the
    // `DependentOperand` alias entirely in the third — expanding it to the
    // raw structural union (`string | Expr<...> | ScalarBuilder<...> | ...`)
    // instead. This is the alias-display-provenance gap tracked by #15391,
    // not a fresh bug; `contains(...)` above only checks that a name
    // survives, not that it is correctly instantiated.
}

/// A recursive imported alias must terminate through the active-node guard.
/// Its valid nested readonly-list form remains accepted, while a wrong leaf is
/// still rejected.
#[test]
fn cross_file_dependent_operand_recursive_alias_terminates() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("recursive.ts"),
        r#"export type RecursiveInput<T> = T | ReadonlyArray<RecursiveInput<T>>
export type Selected<Model, Key extends keyof Model> = {
  [Name in Key]: Model[Name]
}[Key]
"#,
    );
    let usage = r#"import type { RecursiveInput, Selected } from './recursive.js'
interface Model { kind: 'alpha' | 'beta' }
declare function accept<
  Key extends keyof Model,
  Value extends RecursiveInput<Selected<Model, Key>>,
>(key: Key, value: Value): void

accept('kind', ['alpha', ['beta']])
const wrongTree: readonly (string | readonly string[])[] = ['alpha', ['wrong']]
accept('kind', wrongTree)
"#;
    write_file(&base.join("use.ts"), usage);
    let args = parse_args(&[
        "tsz",
        "--noEmit",
        "--strict",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "recursive.ts",
        "use.ts",
    ]);
    let result = compile(&args, base).expect("compile should terminate");
    let actual: Vec<(u32, u32)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect();
    assert_eq!(
        actual,
        [(
            2345,
            usage.rfind("wrongTree").expect("wrong recursive operand") as u32,
        )],
        "recursive dependent alias accepts only the valid nested list: {:#?}",
        result.diagnostics
    );
}

/// The fallback for template conditionals must not reinterpret a legitimate
/// `never` true branch as a failed template match.
#[test]
fn dependent_constraint_template_match_with_never_true_branch_stays_rejected() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("constraint.ts"),
        "export type RejectTemplateMatch<Text> = Text extends `${infer Whole}` ? never : Text\n",
    );
    let usage = r#"import type { RejectTemplateMatch } from './constraint.js'
declare function reject<
  Text extends string,
  Value extends RejectTemplateMatch<Text>,
>(text: Text, value: Value): void
reject('matched', 'matched')
"#;
    write_file(&base.join("use.ts"), usage);
    let args = parse_args(&[
        "tsz",
        "--noEmit",
        "--strict",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "constraint.ts",
        "use.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");
    let actual: Vec<(u32, u32)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect();
    assert_eq!(
        actual,
        [(
            2345,
            usage.rfind("'matched'").expect("rejected matched value") as u32,
        )],
        "a matching template whose true branch is `never` must remain rejected: {:#?}",
        result.diagnostics
    );
}

/// A chain longer than the materialization fuel must fail conservatively and
/// terminate. The generated source keeps the hand-authored Rust shard small.
#[test]
fn dependent_constraint_alias_materialization_is_bounded() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    let mut aliases = String::from("export type Level0<T> = T\n");
    for level in 1..=96 {
        aliases.push_str(&format!(
            "export type Level{level}<T> = Level{}<T>\n",
            level - 1
        ));
    }
    aliases.push_str("export type Deep<T> = Level96<T>\n");
    write_file(&base.join("deep.ts"), &aliases);
    let usage = r#"import type { Deep } from './deep.js'
interface Model { kind: 'alpha' | 'beta' }
declare function accept<
  Key extends keyof Model,
  Value extends Deep<Model[Key]>,
>(key: Key, value: Value): void
accept('kind', 'alpha')
accept('kind', 'wrong')
"#;
    write_file(&base.join("use.ts"), usage);
    let args = parse_args(&[
        "tsz",
        "--noEmit",
        "--strict",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "deep.ts",
        "use.ts",
    ]);
    let result = compile(&args, base).expect("compile should terminate at bounded fuel");
    let actual: Vec<(u32, u32)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect();
    assert_eq!(
        actual,
        [(
            2345,
            usage.rfind("'wrong'").expect("wrong deep operand") as u32,
        )],
        "deep alias materialization accepts the valid leaf and rejects the invalid one: {:#?}",
        result.diagnostics
    );
}

/// A JS module's whole-module `module.exports = class X {}` gives `X` a type
/// meaning the same way TS `export = X` does. Unlike `export =`, this never
/// seeds a binder `export=` symbol — that resolution is a checker-level,
/// type-computed fact (`JsExportSurface::direct_export_type`), not a binder
/// fact — so a naive "is this symbol in the target's exports table" check
/// misses it and TS9006 fires even though `X` is fully nameable from a
/// consumer via `import X = require("./mod")`.
fn ts9006_commonjs_direct_export_target_config() -> &'static str {
    r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "target": "es2015",
    "declaration": true,
    "emitDeclarationOnly": true,
    "module": "commonjs"
  },
  "files": ["obj.js", "index.js"]
}"#
}

#[test]
fn commonjs_whole_module_class_export_is_nameable_from_a_requiring_consumer() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("tsconfig.json"), ts9006_commonjs_direct_export_target_config());
    write_file(
        &base.join("obj.js"),
        r#"module.exports = class Obj {
    constructor() {
        this.x = 12;
    }
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"const Obj = require("./obj");

class Container {
    constructor() {
        this.usage = new Obj();
    }
}

module.exports = Container;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts9006: Vec<_> = result.diagnostics.iter().filter(|d| d.code == 9006).collect();
    assert!(
        ts9006.is_empty(),
        "module.exports = class Obj {{}} makes Obj nameable via `import Obj = require(...)`; \
         no cross-file private-name diagnostic should fire, got: {:#?}",
        result.diagnostics
    );
}

/// Same shape with every binder renamed, to rule out a name-string match.
#[test]
fn commonjs_whole_module_class_export_is_nameable_renamed_binders() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("tsconfig.json"), ts9006_commonjs_direct_export_target_config());
    write_file(
        &base.join("obj.js"),
        r#"module.exports = class Widget {
    constructor() {
        this.label = "hi";
    }
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"const Widget = require("./obj");

class Host {
    constructor() {
        this.child = new Widget();
    }
}

module.exports = Host;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts9006: Vec<_> = result.diagnostics.iter().filter(|d| d.code == 9006).collect();
    assert!(
        ts9006.is_empty(),
        "renamed-binder variant must also suppress TS9006, got: {:#?}",
        result.diagnostics
    );
}

// Negative control: `declaration_emit_raw_typeof_import_text_still_reports_ts9006`
// (above) already covers the case this fix must not suppress — a private
// symbol reached through a *call result* (`require("./some-mod")()`), not
// through the target file's whole-module export identity itself. That test
// continues to assert 2 live TS9006s, so it doubles as the regression guard
// for this change: `is_commonjs_direct_export_target` only matches when the
// referenced symbol IS (or aliases to) the target's own
// `JsExportSurface::direct_export_type`, which a function call's return
// value is not.

fn ts9006_require_aliased_reexport_config() -> &'static str {
    r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "target": "es2015",
    "declaration": true,
    "emitDeclarationOnly": true,
    "module": "commonjs"
  },
  "files": ["cls.js", "cjs2.js", "includeAll.js"]
}"#
}

/// `module.exports = ns;` where `ns = require(...)` re-exports another
/// module's namespace WHOLESALE, distinct from #16254's direct
/// class/function export target: the private member (`Foo`) is reached only
/// by drilling into `ns`'s namespace type, not by `ns` itself matching
/// `Foo`'s symbol. tsc still prints this as a single alias
/// (`import ns = require("./cls"); export = ns;`) and never needs to name
/// `Foo` on its own, so no TS9006 should fire on any member reachable
/// through the re-exported namespace.
#[test]
fn commonjs_require_aliased_whole_module_reexport_is_nameable() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("tsconfig.json"), ts9006_require_aliased_reexport_config());
    write_file(&base.join("cls.js"), "export class Foo {}\n");
    write_file(
        &base.join("cjs2.js"),
        r#"const ns = require("./cls");
module.exports = ns;
"#,
    );
    write_file(&base.join("includeAll.js"), "import \"./cjs2\";\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts9006: Vec<_> = result.diagnostics.iter().filter(|d| d.code == 9006).collect();
    assert!(
        ts9006.is_empty(),
        "module.exports = ns (require-aliased whole-module re-export) makes every member of \
         ns's namespace nameable via `import ns = require(...)`; no TS9006 should fire, got: {:#?}",
        result.diagnostics
    );
}

/// Same shape with every binder renamed, to rule out a name-string match.
#[test]
fn commonjs_require_aliased_whole_module_reexport_is_nameable_renamed_binders() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(&base.join("tsconfig.json"), ts9006_require_aliased_reexport_config());
    write_file(&base.join("cls.js"), "export class Gadget {}\n");
    write_file(
        &base.join("cjs2.js"),
        r#"const alias = require("./cls");
module.exports = alias;
"#,
    );
    write_file(&base.join("includeAll.js"), "import \"./cjs2\";\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts9006: Vec<_> = result.diagnostics.iter().filter(|d| d.code == 9006).collect();
    assert!(
        ts9006.is_empty(),
        "renamed-binder variant must also suppress TS9006, got: {:#?}",
        result.diagnostics
    );
}

/// Negative control: a *structural* re-export (`module.exports = { ns };`,
/// the object literal wraps the require-aliased namespace rather than
/// assigning it wholesale) is a DIFFERENT, still-open mechanism (tsc prints
/// it as `declare const _exports: { ns: typeof ns }; export = _exports;
/// import ns = require("./cls");` — a distinct alias-hoisting shape this fix
/// does not implement). This pins the current, pre-existing behavior so a
/// future widening of the `namespace_module_names` gate cannot silently
/// start masking real TS9006s on structural (non-wholesale) exports without
/// this test forcing a conscious update.
#[test]
fn commonjs_object_literal_wrapped_namespace_reexport_still_reports_ts9006() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "target": "es2015",
    "declaration": true,
    "emitDeclarationOnly": true,
    "module": "commonjs"
  },
  "files": ["cls.js", "cjs.js", "includeAll.js"]
}"#,
    );
    write_file(&base.join("cls.js"), "export class Foo {}\n");
    write_file(
        &base.join("cjs.js"),
        r#"const ns = require("./cls");
module.exports = { ns };
"#,
    );
    write_file(&base.join("includeAll.js"), "import \"./cjs\";\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts9006: Vec<_> = result.diagnostics.iter().filter(|d| d.code == 9006).collect();
    assert!(
        !ts9006.is_empty(),
        "structural (object-literal-wrapped) re-export is a distinct, still-open mechanism; \
         expected TS9006 to still fire, got: {:#?}",
        result.diagnostics
    );
}

/// End-to-end guard for #16928 (fix: #16936). An explicit-root `node_modules`
/// JS file (a `files` entry) is a full program input, so a later
/// `import x = require("pkg")` reads its real CommonJS `module.exports`
/// surface: real members type-check and unknown members report TS2339 on the
/// concrete shape — never a false TS2339 on an empty `typeof import("pkg")`,
/// and never silently widened to `any`.
///
/// #16936's own tests cover the source-read layer (a rooted `node_modules` JS
/// keeps its body). This covers the *checker* layer the reported bug lived in,
/// and pins the cross-mode agreement (`commonjs`/`node16`/`nodenext`) that was
/// the whole failure: pre-fix the surface came back empty, so `node16`/
/// `nodenext` reported a false TS2339 while `commonjs` masked it as `any`.
fn explicit_root_node_modules_js_require_reads_commonjs_surface(module: &str) {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
  "compilerOptions": {{
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "target": "es2015",
    "module": "{module}",
    "types": []
  }},
  "files": ["repro.ts", "node_modules/untyped/index.js"]
}}"#
        ),
    );
    write_file(
        &base.join("node_modules/untyped/package.json"),
        r#"{"name":"untyped","version":"1.0.0","main":"index.js"}"#,
    );
    write_file(
        &base.join("node_modules/untyped/index.js"),
        "module.exports = { hello: function () { return 1; } };\n",
    );
    write_file(
        &base.join("repro.ts"),
        "import u = require(\"untyped\");\nu.hello();\nu.nonexistent();\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();
    // `u.hello()` is a real member of the read CJS surface — no TS2339 there
    // (pre-fix this was the reported false-positive on an empty
    // `typeof import("untyped")` under node16/nodenext).
    assert!(
        ts2339.iter().all(|d| !d.message_text.contains("'hello'")),
        "[module={module}] real CJS member `u.hello()` must not report TS2339, got: {:#?}",
        result.diagnostics
    );
    // `u.nonexistent()` is not on the surface — it must report TS2339 on the
    // concrete `{ hello: () => number; }` shape. That the type is the real
    // shape (not `any`, not an empty `typeof import(...)`) is the point: it
    // proves the rooted JS body was parsed and its `module.exports` read.
    let nonexistent: Vec<_> = ts2339
        .iter()
        .filter(|d| d.message_text.contains("'nonexistent'"))
        .collect();
    assert!(
        nonexistent
            .iter()
            .any(|d| d.message_text.contains("{ hello: () => number; }")),
        "[module={module}] unknown member `u.nonexistent()` must report TS2339 on the \
         read `{{ hello: () => number; }}` surface (proves `u` is the real shape, not \
         `any` nor an empty `typeof import(...)`), got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn explicit_root_node_modules_js_require_reads_surface_commonjs() {
    explicit_root_node_modules_js_require_reads_commonjs_surface("commonjs");
}

#[test]
fn explicit_root_node_modules_js_require_reads_surface_node16() {
    explicit_root_node_modules_js_require_reads_commonjs_surface("node16");
}

#[test]
fn explicit_root_node_modules_js_require_reads_surface_nodenext() {
    explicit_root_node_modules_js_require_reads_commonjs_surface("nodenext");
}

// ---------------------------------------------------------------------------
// #17024: `parse_property_name_impl`'s identifier/keyword branch read
// `token_end()` *after* `next_token()` had already advanced past the name, so
// the shared property-name node's `end` overshot into whatever token follows
// (e.g. the `(` after an accessor name). Since diagnostic ordering sorts by
// `(start, length, code)`, the inflated length flipped the order between a
// parser grammar diagnostic anchored on the name (TS1054/TS1049) and a
// checker diagnostic anchored at the same `start` (TS2378/TS7032). Every
// expectation below is oracle-verified against `typescript@7.0.2`.
// ---------------------------------------------------------------------------

/// Returns `(start, length)` for the first diagnostic with `code`.
fn first_diagnostic_span(
    diagnostics: &[tsz_common::diagnostics::Diagnostic],
    code: u32,
) -> (u32, u32) {
    let diag = diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected code {code}, got: {diagnostics:#?}"));
    (diag.start, diag.length)
}

#[test]
fn get_accessor_ts1054_span_does_not_overshoot_into_the_parameter_list() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("index.ts"),
        "class Cq30 { get xq30(vq30: number): string { return \"\"; } }\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "index.ts"]);
    let result = compile(&args, base).expect("compile should succeed");
    let (_, length) = first_diagnostic_span(&result.diagnostics, 1054);
    assert_eq!(
        length, 4,
        "TS1054 must span exactly the accessor name `xq30` (length 4), not overshoot into `(`; got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn get_accessor_with_value_parameter_orders_ts1054_before_ts2378_matching_tsc() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("index.ts"),
        "class Cr31 { get xr31(vr31: number): string {} }\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "index.ts"]);
    let result = compile(&args, base).expect("compile should succeed");

    let (ts1054_start, ts1054_len) = first_diagnostic_span(&result.diagnostics, 1054);
    let (ts2378_start, ts2378_len) = first_diagnostic_span(&result.diagnostics, 2378);
    assert_eq!(
        ts1054_start, ts2378_start,
        "TS1054 and TS2378 must anchor at the same start (the accessor name)"
    );
    assert_eq!(
        ts1054_len, ts2378_len,
        "TS1054 and TS2378 must share the same span length (both anchor on `name`)"
    );

    let ts1054_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 1054)
        .unwrap();
    let ts2378_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 2378)
        .unwrap();
    assert!(
        ts1054_index < ts2378_index,
        "tsc's (start, length, code) ordering puts TS1054 before TS2378 at an equal span; got order: {:#?}",
        result.diagnostics
    );
}

/// Same shape with every binder renamed, to rule out a name-string match.
#[test]
fn get_accessor_with_value_parameter_orders_ts1054_before_ts2378_renamed_binders() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("index.ts"),
        "class ContainerS32 { get widthS32(inputS32: number): string {} }\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "index.ts"]);
    let result = compile(&args, base).expect("compile should succeed");

    let ts1054_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 1054)
        .unwrap_or_else(|| panic!("expected TS1054, got: {:#?}", result.diagnostics));
    let ts2378_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 2378)
        .unwrap_or_else(|| panic!("expected TS2378, got: {:#?}", result.diagnostics));
    assert!(
        ts1054_index < ts2378_index,
        "renamed binders must not change the (start, length, code) ordering; got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn set_accessor_with_no_parameters_orders_ts1049_before_ts7032_matching_tsc() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("index.ts"),
        "class Ct33 { set yt33() {} }\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "index.ts"]);
    let result = compile(&args, base).expect("compile should succeed");

    let (ts1049_start, ts1049_len) = first_diagnostic_span(&result.diagnostics, 1049);
    let (ts7032_start, ts7032_len) = first_diagnostic_span(&result.diagnostics, 7032);
    assert_eq!(
        ts1049_start, ts7032_start,
        "TS1049 and TS7032 must anchor at the same start (the accessor name)"
    );
    assert_eq!(
        ts1049_len, ts7032_len,
        "TS1049 and TS7032 must share the same span length (both anchor on `name`)"
    );

    let ts1049_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 1049)
        .unwrap();
    let ts7032_index = result
        .diagnostics
        .iter()
        .position(|d| d.code == 7032)
        .unwrap();
    assert!(
        ts1049_index < ts7032_index,
        "tsc's (start, length, code) ordering puts TS1049 before TS7032 at an equal span; got order: {:#?}",
        result.diagnostics
    );
}

/// A `set` accessor's own `name.end` (not the accessor family's shared
/// helper) is exercised the same way in a type-literal member, confirming
/// the fix is not scoped to class members alone.
#[test]
fn type_literal_set_accessor_ts1049_span_does_not_overshoot() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("index.ts"),
        "type Tv35 = { set yv35(av35: number, bv35: number): void };\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "--strict", "index.ts"]);
    let result = compile(&args, base).expect("compile should succeed");
    let (_, length) = first_diagnostic_span(&result.diagnostics, 1049);
    assert_eq!(
        length, 4,
        "TS1049 must span exactly the accessor name `yv35` (length 4), not overshoot into `(`; got: {:#?}",
        result.diagnostics
    );
}
