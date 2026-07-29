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
    let expected = vec![
        (2345, usage.find("'wrong'").expect("wrong operand") as u32),
        (
            2345,
            usage
                .find("['one', 'wrong']")
                .expect("wrong mixed-list operand") as u32,
        ),
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
            && result.diagnostics[1].message_text.contains("DependentOperand")
            && result.diagnostics[1].message_text.contains("category")
            && result.diagnostics[2].message_text.contains("DependentOperand")
            && result.diagnostics[2].message_text.contains("title"),
        "dependent-constraint diagnostics must retain their selected field: {:#?}",
        result.diagnostics
    );
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

/// Dynamic `import()` uses the same runtime namespace surface as a namespace
/// import: native values receive value-side augmentations, augmentation-only
/// namespaces are present, and pure type additions remain absent.
#[test]
fn dynamic_import_uses_runtime_module_augmentation_surface() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("native.ts"),
        r#"export declare function f(value: string): "s";
export interface NativeInterface { native: true }
export interface NativeTypeOnly { hidden: true }
"#,
    );
    write_file(
        &base.join("augmentation.ts"),
        r#"import "./native";
declare module "./native" {
    function f(value: number): "n";
    namespace f {
        const meta: "function-meta";
    }
    namespace AddedRuntime {
        const meta: "added-meta";
    }
    interface AddedTypeOnly {
        hidden: true;
    }
    namespace NativeInterface {
        const meta: "interface-meta";
    }
}
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"import "./augmentation";
async function inspect() {
    const mod = await import("./native");

    const nativeCall: "s" = mod.f("text");
    const augmentedCall: "n" = mod.f(1);
    const functionMeta: "function-meta" = mod.f.meta;
    const addedMeta: "added-meta" = mod.AddedRuntime.meta;
    const interfaceMeta: "interface-meta" = mod.NativeInterface.meta;

    const wrongReturn: "n" = mod.f("text");
    mod.f(true);
    mod.AddedTypeOnly;
}
"#,
    );

    let args = parse_args(&[
        "tsz",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "native.ts",
        "augmentation.ts",
        "consumer.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");
    let actual: Vec<u32> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        actual,
        [2322, 2769, 2339],
        "dynamic import must expose only the augmented runtime namespace surface: {:#?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == 2339)
            .is_some_and(|diagnostic| {
                diagnostic.message_text.contains("typeof import(")
                    && diagnostic.message_text.contains("native")
            }),
        "the missing type-only addition must retain namespace import display: {:#?}",
        result.diagnostics
    );
}
