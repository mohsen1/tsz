#[test]
fn array_to_enum_name_does_not_override_declared_return_type() {
    let source = r#"
declare function arrayToEnum<T extends string>(values: readonly T[]): Record<T, number>;

const values = arrayToEnum(["A", "B"] as const);

const numberValue: number = values.A;
const literalValue: "A" = values.A;

type Values = typeof values;
type AType = Values["A"];

const numberFromType: AType = 1;
const literalFromType: AType = "A";
"#;
    let diags = diagnostics_for_source(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        ts2322
            .iter()
            .any(|msg| msg.contains("Type 'number' is not assignable to type '\"A\"'")),
        "expected declared Record return type for value access; got: {diags:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|msg| msg.contains("Type 'string' is not assignable to type 'number'")),
        "expected declared Record return type for typeof/indexed access; got: {diags:#?}"
    );
    assert!(
        !ts2322
            .iter()
            .any(|msg| msg.contains("Type '1' is not assignable to type '\"A\"'")),
        "arrayToEnum shortcut should not fabricate literal member values; got: {diags:#?}"
    );
}

#[test]
fn scoped_destructured_typeof_indexed_access_uses_binding_type() {
    let source = r#"
type IsolationLevel = 'read committed' | 'serializable';
type Tedious = { ISOLATION_LEVEL: Record<string, number> };

class Driver {
  #tedious: Tedious;

  constructor(tedious: Tedious) {
    this.#tedious = tedious;
  }

  getLevel(isolationLevel: IsolationLevel): number {
    const { ISOLATION_LEVEL } = this.#tedious;
    const mapper: Record<
      IsolationLevel,
      (typeof ISOLATION_LEVEL)[keyof typeof ISOLATION_LEVEL]
    > = {
      'read committed': ISOLATION_LEVEL.READ_COMMITTED,
      serializable: ISOLATION_LEVEL.SERIALIZABLE,
    };

    return mapper[isolationLevel];
  }
}
"#;
    let diags = diagnostics_for_source(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected local destructured typeof indexed access to resolve to number; got: {diags:#?}"
    );
    assert!(
        !has_diagnostic_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
        ),
        "expected mapped Record keys to stay concrete; got: {diags:#?}"
    );
}

#[test]
fn imported_destructured_typeof_indexed_access_uses_binding_type() {
    let config = r#"
export type Tedious = { ISOLATION_LEVEL: Record<string, number> };
"#;
    let driver = r#"
import type { Tedious } from './config';

type IsolationLevel = 'read committed' | 'serializable';

class Driver {
  #tedious: Tedious;

  constructor(tedious: Tedious) {
    this.#tedious = tedious;
  }

  getLevel(isolationLevel: IsolationLevel): number {
    const { ISOLATION_LEVEL } = this.#tedious;
    const mapper: Record<
      IsolationLevel,
      (typeof ISOLATION_LEVEL)[keyof typeof ISOLATION_LEVEL]
    > = {
      'read committed': ISOLATION_LEVEL.READ_COMMITTED,
      serializable: ISOLATION_LEVEL.SERIALIZABLE,
    };

    return mapper[isolationLevel];
  }
}
"#;
    let diags = tsz_checker::test_utils::check_multi_file(
        &[("./config.ts", config), ("./driver.ts", driver)],
        "./driver.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected imported destructured typeof indexed access to resolve to number; got: {diags:#?}"
    );
    assert!(
        !has_diagnostic_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
        ),
        "expected imported mapped Record keys to stay concrete; got: {diags:#?}"
    );
}

#[test]
fn imported_array_item_type_from_const_array_preserves_literals() {
    let type_utils_padding: String = (0..140)
        .map(|idx| format!("export type PaddingAlias{idx} = {{ value: {idx} }};\n"))
        .collect();
    let type_utils = format!(
        r#"
{type_utils_padding}
export type ArrayItemType<T> = T extends ReadonlyArray<infer I> ? I : never;
"#
    );
    let driver = r#"
import type { ArrayItemType } from '../util/type-utils.js';

export const TRANSACTION_ISOLATION_LEVELS = [
  'read uncommitted',
  'read committed',
  'repeatable read',
  'serializable',
  'snapshot',
] as const;

export type IsolationLevel = ArrayItemType<typeof TRANSACTION_ISOLATION_LEVELS>;

export interface TransactionSettings {
  readonly accessMode?: AccessMode;
  readonly isolationLevel?: IsolationLevel;
}

export const TRANSACTION_ACCESS_MODES = ['read only', 'read write'] as const;

export type AccessMode = ArrayItemType<typeof TRANSACTION_ACCESS_MODES>;

export function validateTransactionSettings(settings: TransactionSettings): void {
  if (
    settings.accessMode &&
    !TRANSACTION_ACCESS_MODES.includes(settings.accessMode)
  ) {
    throw new Error(`invalid transaction access mode ${settings.accessMode}`);
  }

  if (
    settings.isolationLevel &&
    !TRANSACTION_ISOLATION_LEVELS.includes(settings.isolationLevel)
  ) {
    throw new Error(`invalid transaction isolation level ${settings.isolationLevel}`);
  }
}
"#;
    let type_error = r#"
export type KyselyTypeError<E extends string> = { __error__: E } & never;
"#;
    let config = r#"
import type { KyselyTypeError } from '../../util/type-error.js';

export interface Tedious {
  connectionFactory: () => TediousConnection | Promise<TediousConnection>;
  ISOLATION_LEVEL: TediousIsolationLevel;
  Request: TediousRequestClass;
  resetConnectionOnRelease?: KyselyTypeError<'deprecated'>;
  TYPES: TediousTypes;
}
export type TediousIsolationLevel = Record<string, number>;
export type TediousConnection = {
  beginTransaction(
    callback: (err: Error | null | undefined) => void,
    name?: string | undefined,
    isolationLevel?: number | undefined,
  ): void;
  connect(connectListener: (err?: Error) => void): void;
  on(event: 'error', listener: (error: unknown) => void): this;
  on(event: string, listener: (...args: any[]) => void): this;
};
export interface TediousRequestClass {
  new (
    sqlTextOrProcedure: string | undefined,
    callback: (error?: Error | null, rowCount?: number, rows?: any) => void,
    options?: { statementColumnEncryptionSetting?: any },
  ): TediousRequest;
}
export declare class TediousRequest {}
export interface TediousTypes {
  NVarChar: TediousDataType;
  [x: string]: TediousDataType;
}
export interface TediousDataType {}
"#;
    let database_connection = r#"
import type { TransactionSettings } from './driver.js';

export interface DatabaseConnection {
  beginTransaction(settings: TransactionSettings): Promise<void>;
}
"#;
    let usage = r#"
import type { IsolationLevel, TransactionSettings } from '../../driver/driver.js';
import type { DatabaseConnection } from '../../driver/database-connection.js';
import type { Tedious, TediousConnection } from './mssql-dialect-config.js';

const LOCAL_TRANSACTION_ISOLATION_LEVELS = [
  'read uncommitted',
  'read committed',
  'repeatable read',
  'serializable',
  'snapshot',
] as const;

class Driver implements DatabaseConnection {
  #tedious: Tedious;
  #connection: TediousConnection;

  constructor(tedious: Tedious, connection: TediousConnection) {
    this.#tedious = tedious;
    this.#connection = connection;
  }

  async beginTransaction(settings: TransactionSettings): Promise<void> {
    const { isolationLevel } = settings;

    await new Promise((resolve, reject) =>
      this.#connection.beginTransaction(
        (error) => {
          if (error) reject(error);
          else resolve(undefined);
        },
        isolationLevel ? 'tx' : undefined,
        isolationLevel ? this.#getTediousIsolationLevel(isolationLevel) : undefined,
      ),
    );
  }

  #getTediousIsolationLevel(isolationLevel: IsolationLevel) {
    if (!LOCAL_TRANSACTION_ISOLATION_LEVELS.includes(isolationLevel)) {
      throw new Error(`invalid transaction isolation level ${isolationLevel}`);
    }

    const { ISOLATION_LEVEL } = this.#tedious;

    const mapper: Record<
      IsolationLevel,
      (typeof ISOLATION_LEVEL)[keyof typeof ISOLATION_LEVEL]
    > = {
      'read committed': ISOLATION_LEVEL.READ_COMMITTED,
      'read uncommitted': ISOLATION_LEVEL.READ_UNCOMMITTED,
      'repeatable read': ISOLATION_LEVEL.REPEATABLE_READ,
      serializable: ISOLATION_LEVEL.SERIALIZABLE,
      snapshot: ISOLATION_LEVEL.SNAPSHOT,
    };

    const tediousIsolationLevel = mapper[isolationLevel];
    if (tediousIsolationLevel === undefined) {
      throw new Error(`Unknown isolation level: ${isolationLevel}`);
    }

    return tediousIsolationLevel;
  }
}

const bad: IsolationLevel = 'nope';
const badNumber: IsolationLevel = 1;
"#;
    let driver_diags = tsz_checker::test_utils::check_multi_file(
        &[
            ("./src/util/type-error.ts", type_error),
            ("./src/util/type-utils.ts", type_utils.as_str()),
            ("./src/driver/driver.ts", driver),
        ],
        "./src/driver/driver.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(&driver_diags, 2345),
        "expected imported const-array item type to be accepted by includes() in defining module; got: {driver_diags:#?}"
    );

    let diags = tsz_checker::test_utils::check_multi_file(
        &[
            ("./src/util/type-error.ts", type_error),
            ("./src/util/type-utils.ts", type_utils.as_str()),
            ("./src/driver/driver.ts", driver),
            ("./src/driver/database-connection.ts", database_connection),
            ("./src/dialect/mssql/mssql-dialect-config.ts", config),
            ("./src/dialect/mssql/mssql-driver.ts", usage),
        ],
        "./src/dialect/mssql/mssql-driver.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected imported const-array item type to reject unrelated literals; got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .count()
            >= 2,
        "expected imported const-array item type to reject string and number literals; got: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.message_text.contains("Record<IsolationLevel")
                && diag.message_text.contains("[number]")
        }),
        "expected indexing Record<IsolationLevel, number> by IsolationLevel to be number; got: {diags:#?}"
    );
    assert!(
        !has_diagnostic_code(&diags, 2345),
        "expected imported const-array item type to be accepted by includes(); got: {diags:#?}"
    );
    assert!(
        !has_diagnostic_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
        ),
        "expected Record keys from imported const-array item type; got: {diags:#?}"
    );
}

// =============================================================================
// Type alias instantiation with template-literal interpolation (TS2322)
// =============================================================================
//
// When a type alias parameter only appears inside a template-literal
// interpolation, variance is structurally unreliable: stringification can make
// `\`a${number}\`` a subtype of `\`a${string}\`` even though `number` is not a
// subtype of `string`. The structural assignability check (which compares the
// expanded property types) is the authoritative signal; the same-base
// type-alias rejection guard must defer to it instead of forcing strict
// covariance on the unreliable type argument.

#[test]
fn ts2322_template_literal_alias_arg_does_not_force_covariant_rejection() {
    let source = r#"
        type AGen<T extends string | number> = { field: `a${T}` };
        const ok1: AGen<string> = null as any as AGen<"yes">;
        const ok2: AGen<string> = null as any as AGen<number>;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322.is_empty(),
        "AGen<number>/AGen<\"yes\"> should be assignable to AGen<string> via template-literal stringification, got: {ts2322:#?}"
    );
}

#[test]
fn ts2322_template_literal_alias_alt_param_name_still_passes() {
    // Same structural rule, different parameter name — proves the fix is not
    // keyed on user-chosen identifiers.
    let source = r#"
        type Wrap<K extends string | number> = { field: `a${K}` };
        const ok: Wrap<string> = null as any as Wrap<number>;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        ts2322.is_empty(),
        "Wrap<number> should be assignable to Wrap<string> regardless of param name, got: {ts2322:#?}"
    );
}

#[test]
fn ts2322_non_template_alias_still_rejects_covariant_mismatch() {
    // Sanity check: when the type parameter does NOT live inside a template
    // literal, variance IS reliable and the rejection guard should still bite
    // for genuinely incompatible covariant arguments.
    let source = r#"
        type Box<T> = { value: T };
        const bad: Box<string> = null as any as Box<number>;
    "#;

    let diagnostics = diagnostics_for_source(source);
    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Box<number> should NOT be assignable to Box<string>, got: {diagnostics:#?}"
    );
}

// =============================================================================
// Missing string index signature — interface/class vs indexed target
// =============================================================================

#[test]
fn ts2322_interface_without_index_sig_not_assignable_to_string_indexed_type() {
    let source = r#"
        interface StringIndex { [key: string]: number }
        interface SpecificProps { a: number; b: number }
        const idx: StringIndex = { a: 1, b: 2 } as SpecificProps;
    "#;
    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn ts2322_interface_without_index_sig_via_variable_not_assignable_to_string_indexed_type() {
    // Variable binding (not type assertion) also requires index signature.
    let source = r#"
        interface StringIndex { [key: string]: number }
        interface Counts { x: number; y: number }
        declare const counts: Counts;
        const idx: StringIndex = counts;
    "#;
    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn ts2322_interface_with_matching_index_sig_is_assignable_to_string_indexed_type() {
    // Baseline: an interface that already declares the matching index signature is fine.
    let source = r#"
        interface StringIndex { [key: string]: number }
        interface Indexed { [key: string]: number; a: number; b: number }
        declare const x: Indexed;
        const idx: StringIndex = x;
    "#;
    assert!(!has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn ts2322_fresh_object_literal_with_compatible_props_is_assignable_to_string_indexed_type() {
    // Fresh object literals are assignable even without an explicit index sig.
    let source = r#"
        interface StringIndex { [key: string]: number }
        const idx: StringIndex = { a: 1, b: 2 };
    "#;
    assert!(!has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

// =============================================================================
// Issue #5887: optional generic `|| {}` / `?? {}` not assignable to `object`
// Structural rule: when (T | undefined) || X or (T | undefined) ?? X where T
// is an unconstrained type parameter, tsc produces (T & {}) | X (the
// non-nullable intersection of T). For X = {}, this reduces to {} which IS
// assignable to `object`. Any name for T must work (generalization check).
// =============================================================================

#[test]
fn test_ts2322_no_false_positive_optional_generic_or_empty_object_as_object_return() {
    // function test<D>(input?: D): object { return input || {}; }
    // TSC: OK (no TS2322). The `||` result is `D & {} | {}` = `{}` after
    // non-nullable type-parameter reduction; `{}` is assignable to `object`.
    let source = r#"
        function test<D>(input?: D): object {
            return input || {};
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `(D | undefined) || {{}}` returned as `object`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_optional_generic_nullish_coalesce_empty_object_as_object() {
    // function test<D>(input?: D): object { return input ?? {}; }
    // The `??` operator also applies the non-nullable approximation.
    let source = r#"
        function test<D>(input?: D): object {
            return input ?? {};
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `(D | undefined) ?? {{}}` returned as `object`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_optional_generic_name_invariant() {
    // The fix must not be keyed on the type-parameter name.
    // Use three different names to verify generality.
    let source = r#"
        function withT<T>(x?: T): object { return x || {}; }
        function withK<K>(x?: K): object { return x || {}; }
        function withValue<Value>(x?: Value): object { return x || {}; }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for optional generic `|| {{}}` with various type-param names, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_generic_null_or_empty_object_as_object() {
    // function test<D>(input: D | null): object { return input || {}; }
    // null-union instead of undefined-union — same rule applies.
    let source = r#"
        function test<D>(input: D | null): object {
            return input || {};
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `(D | null) || {{}}` returned as `object`, got: {diags:?}"
    );
}

#[test]
fn test_ts2322_optional_generic_or_primitive_fallback_still_errors() {
    // function test<D>(input?: D): string { return input || "hello"; }
    // D is unconstrained, so `D & {}` is not assignable to `string`.
    // This should still be a TS2322 error.
    let source = r#"
        function test<D>(input?: D): string {
            return input || "hello";
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for `(D | undefined) || \"hello\"` returned as `string`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_constrained_to_object_optional_generic_no_false_positive() {
    // function test<D extends object>(x?: D): object { return x || {}; }
    // D extends object so D is definitely assignable to object; the whole
    // pattern should still compile cleanly.
    let source = r#"
        function test<D extends object>(x?: D): object {
            return x || {};
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `(D extends object | undefined) || {{}}`, got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_explicit_union_undefined_or_empty_object_as_object_assignment() {
    // Explicit `D | undefined` parameter assigned via `||` fallback to `object`:
    // tsc accepts this because the truthy branch produces `D & {}`, which IS
    // assignable to the `object` keyword even for an unconstrained type param.
    let source = r#"
        function test<D>(data: D | undefined): object {
            let d: object = data || {};
            return d;
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for explicit `(D | undefined) || {{}}` assigned to `object`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_explicit_union_undefined_or_empty_object_various_names() {
    // Verify name-invariance for the explicit `D | undefined` variant.
    let source = r#"
        function withT<T>(x: T | undefined): object {
            let r: object = x || {};
            return r;
        }
        function withValue<Value>(x: Value | undefined): object {
            let r: object = x || {};
            return r;
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for explicit `(T | undefined) || {{}}` assignment with various names, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_multi_type_param_union_undefined_or_empty_object() {
    // Structural rule: when `D | E | undefined || {}` is used, where D and E are
    // both unconstrained type parameters, the result should be assignable to `object`
    // because each type param gets the `& {}` treatment making the union object-safe.
    let source = r#"
        function withTwo<D, E>(x: D | E | undefined): object {
            let r: object = x || {};
            return r;
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for `(D | E | undefined) || {{}}` assigned to `object`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_class_method_generic_or_empty_object_as_object() {
    // The `(D | undefined) || {}` → `object` rule applies in class method contexts too.
    let source = r#"
        class Foo<D> {
            method(data: D | undefined): object {
                let d: object = data || {};
                return d;
            }
        }
    "#;
    let diags = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for class method `(D | undefined) || {{}}` assigned to `object`, \
         got: {diags:?}"
    );
}

#[test]
fn test_ts2322_generic_alias_chain_reduces_to_application_for_infer() {
    // Structural rule: when matching `Application(B, args)` against
    // pattern `Application(B_pat, [infer V])` and `B` is a generic type
    // alias whose body is itself an `Application(B_pat, [X])`, peel one
    // alias step so bases align and `V` binds to the substituted `X`.
    let source = r#"
        type Cond<P> = P extends Promise<infer T> ? T : never;
        type ToPromise<X> = Promise<X>;

        type R = Cond<ToPromise<{ id: number }>>;
        const ok: R = { id: 1 };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for Cond<ToPromise<{{id}}>>: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_generic_alias_chain_renamed_infer_var() {
    // Anti-hardcoding: the rule must hold regardless of the infer variable
    // name (`T` vs `P`) and the alias parameter name (`X` vs `Y`).
    let source = r#"
        type Cond<Q> = Q extends Promise<infer P> ? P : never;
        type Wrap<Y> = Promise<Y>;

        type R = Cond<Wrap<string>>;
        const ok: R = "hello";
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for renamed-infer Cond<Wrap<string>>: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_multi_layer_generic_alias_chain() {
    // Two layers of generic aliasing must all peel back to Promise.
    let source = r#"
        type Inner<X> = Promise<X>;
        type Outer<Y> = Inner<Y>;
        type Cond<P> = P extends Promise<infer T> ? T : never;

        type R = Cond<Outer<{ id: number }>>;
        const ok: R = { id: 1 };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for two-layer alias chain: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_async_return_via_return_type_promise_infer() {
    // Reported repro from issue #6581: when the alias body is a Conditional
    // (ReturnType's body) that yields `Application(Promise, ...)` via infer,
    // the outer conditional must still recover the Application form for
    // `Promise<infer T>` to bind `T`.
    let source = r#"
        type AsyncReturn<F extends (...args: any) => any> =
            ReturnType<F> extends Promise<infer T> ? T : never;

        declare function fetchUser(): Promise<{ id: number }>;

        type FU = AsyncReturn<typeof fetchUser>;
        const fu: FU = { id: 1 };
    "#;
    let diags = diagnostics_for_source(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for AsyncReturn<typeof fetchUser>: {diags:?}"
    );
}

#[test]
fn test_ts2322_unwrap_over_return_type_alias() {
    // Variant: the source is `Unwrap<R>` where `R` is a non-generic alias
    // for `ReturnType<typeof f>`. Same conditional-body reduction must apply.
    let source = r#"
        type Unwrap<P> = P extends Promise<infer X> ? X : never;
        declare function getUser(): Promise<{ id: number }>;

        type R = ReturnType<typeof getUser>;
        type U = Unwrap<R>;
        const u: U = { id: 1 };
    "#;
    let diags = diagnostics_for_source(source);
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for Unwrap<R> via ReturnType alias: {diags:?}"
    );
}

#[test]
fn awaited_return_type_object_literal_reports_all_property_mismatches() {
    let source = r#"
        async function fetchData(): Promise<{ id: number; name: string }> {
            return { id: 1, name: "test" };
        }

        type AwaitedData = Awaited<ReturnType<typeof fetchData>>;
        const wrongAwaitedData: AwaitedData = { id: "wrong", name: 123 };
    "#;
    let diags = diagnostics_for_source(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322.len(),
        2,
        "expected both Awaited<ReturnType<...>> property mismatches, got: {ts2322:#?}",
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("Type 'string' is not assignable to type 'number'")),
        "expected id mismatch diagnostic, got: {ts2322:#?}",
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("Type 'number' is not assignable to type 'string'")),
        "expected name mismatch diagnostic, got: {ts2322:#?}",
    );
}

#[test]
fn mapped_type_object_literal_reports_all_property_mismatches() {
    let source = r#"
        type Nullable<T> = { [K in keyof T]: T[K] | null };

        interface User {
            name: string;
            age: number;
        }

        type NullableUser = Nullable<User>;
        const wrongU: NullableUser = { name: 42, age: "hello" };
    "#;
    let diags = diagnostics_for_source(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322.len(),
        2,
        "expected both mapped type property mismatches, got: {ts2322:#?}",
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("Type 'number' is not assignable to type 'string'")),
        "expected name mismatch diagnostic, got: {ts2322:#?}",
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("Type 'string' is not assignable to type 'number'")),
        "expected age mismatch diagnostic, got: {ts2322:#?}",
    );
}

#[test]
fn test_ts2322_generic_alias_chain_inline_vs_alias_parity() {
    // Generalization gate: peeling must not regress the no-alias path.
    // `Cond<Promise<X>>` (inline) and `Cond<ToPromise<X>>` (aliased) must
    // both bind `T` to `X`.
    let source = r#"
        type Cond<P> = P extends Promise<infer T> ? T : never;
        type ToPromise<X> = Promise<X>;

        type Inline = Cond<Promise<{ id: number }>>;
        type Aliased = Cond<ToPromise<{ id: number }>>;
        const inline: Inline = { id: 1 };
        const aliased: Aliased = { id: 1 };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322: Vec<_> =
        diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322.is_empty(),
        "Expected inline and aliased Cond to behave identically: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_generic_alias_chain_negative_non_promise_takes_false_branch() {
    // Negative: ensure peeling does NOT cause a false positive in the
    // false branch. When the source is not Promise-shaped at all, the
    // conditional must take the false branch and the result type must
    // reject Promise-shape assignments.
    let source = r#"
        type Unwrap<P> = P extends Promise<infer X> ? X : "fallback";
        type NotPromise = { x: number };
        type U = Unwrap<NotPromise>;
        const ok: U = "fallback";
        const bad: U = { x: 1 };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322 >= 1,
        "Expected the `bad` line to error (U is 'fallback'), got {ts2322} TS2322 diagnostics: {diags:?}"
    );
}

#[test]
fn test_ts2322_async_return_via_return_type_negative_sync_function() {
    // Negative: a synchronous function should take the `never` branch.
    // Assigning a value-shape to `never` must still error.
    let source = r#"
        type AsyncReturn<F extends (...args: any) => any> =
            ReturnType<F> extends Promise<infer T> ? T : never;
        declare function syncFn(): { id: number };
        type FU = AsyncReturn<typeof syncFn>;
        const bad: FU = { id: 1 };
    "#;
    let diags = diagnostics_for_source(source);
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 when assigning to AsyncReturn of a sync function: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_extends_never_return_no_false_positive() {
    // `T extends never` → T can only be `never`, so returning T as `never` is valid.
    let source = r#"
        function handleT<T extends never>(x: T): never { return x; }
        function handleN<N extends never>(x: N): never { return x; }
        function handleK<K extends never>(x: K): never { return x; }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 for `T extends never` return: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_extends_never_variable_no_false_positive() {
    // Assigning a value of type T (extends never) to a variable of type never is valid.
    let source = r#"
        function f<T extends never>(x: T): T {
            const y: never = x;
            return y;
        }
        function g<U extends never>(x: U): U {
            const z: never = x;
            return z;
        }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 when assigning `T extends never` to `never`: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_extends_never_transitive_constraint() {
    // T extends U where U extends never → T should also be assignable to never.
    let source = r#"
        function passDown<U extends never, T extends U>(x: T): never { return x; }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 for transitive `T extends U extends never`: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_assigns_to_recursive_interface_constraint() {
    let source = r#"
        interface MyNode<T> {
            value: T;
            child?: MyNode<T>;
        }
        function f<T extends MyNode<number>>(t: T): MyNode<number> {
            return t;
        }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 when returning a type parameter through its recursive interface constraint: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_assigns_to_aliased_recursive_interface_constraint() {
    let source = r#"
        interface TreeBox<U> {
            item: U;
            next?: TreeBox<U>;
        }
        type NumberTree = TreeBox<number>;
        function f<X extends NumberTree>(x: X): NumberTree {
            return x;
        }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 when returning a type parameter through an aliased recursive interface constraint: {diags:?}"
    );
}

#[test]
fn test_ts2322_type_param_assigns_through_transitive_recursive_constraint() {
    let source = r#"
        interface Link<V> {
            value: V;
            child?: Link<V>;
        }
        function f<Base extends Link<number>, Item extends Base>(item: Item): Link<number> {
            return item;
        }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322 when returning through a transitive recursive interface constraint: {diags:?}"
    );
}

#[test]
fn test_ts2322_incompatible_recursive_interface_constraint_still_errors() {
    let source = r#"
        interface NumberNode {
            value: number;
            child?: NumberNode;
        }
        interface TextNode {
            value: string;
            child?: TextNode;
        }
        function f<T extends TextNode>(t: T): NumberNode {
            return t;
        }
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322 >= 1,
        "Expected TS2322 when a recursive constraint is structurally incompatible with the target: {diags:?}"
    );
}

#[test]
fn test_ts2322_fbounded_object_literal_empty_array_no_error() {
    let source = r#"
        interface TreeNodeBase<T extends TreeNodeBase<T>> {
            parent: TreeNodeBase<T> | null;
            children: T[];
        }
        interface FileEntry extends TreeNodeBase<FileEntry> {
            name: string;
        }
        const root: FileEntry = { name: "root", parent: null, children: [] };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322: empty array in F-bounded object literal should adopt contextual type: {diags:?}"
    );

    let libs = load_lib_files_for_test();
    if !libs.is_empty() {
        let diags = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
        let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
        assert_eq!(
            ts2322, 0,
            "Expected no TS2322 with lib files: empty array in F-bounded object literal should adopt contextual type: {diags:?}"
        );
    }
}

#[test]
fn test_ts2322_fbounded_object_literal_empty_array_renamed_param() {
    let source = r#"
        interface NodeBase<U extends NodeBase<U>> {
            parent: NodeBase<U> | null;
            children: U[];
        }
        interface TreeItem extends NodeBase<TreeItem> {
            label: string;
        }
        const root: TreeItem = { label: "root", parent: null, children: [] };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322: empty array in F-bounded (U param) object literal should adopt contextual type: {diags:?}"
    );

    let libs = load_lib_files_for_test();
    if !libs.is_empty() {
        let diags = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
        let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
        assert_eq!(
            ts2322, 0,
            "Expected no TS2322 with lib files (U param): {diags:?}"
        );
    }
}

#[test]
fn test_ts2322_fbounded_wrong_element_type_errors() {
    let source = r#"
        interface TreeNodeBase<T extends TreeNodeBase<T>> {
            children: T[];
        }
        interface FileEntry extends TreeNodeBase<FileEntry> {
            name: string;
        }
        const root: FileEntry = { name: "root", children: [42] };
    "#;
    let diags = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322 >= 1,
        "Expected TS2322: number is not assignable to FileEntry: {diags:?}"
    );
}

const TREE_BTREE_INTERFACES: &str = r#"
        interface Tree<T extends Tree<T>> {
            children: T[];
        }
        interface BTree extends Tree<BTree> {
            value: number;
        }
    "#;

#[test]
fn test_ts2322_fbounded_no_parent_field_empty_array_no_error() {
    // Minimal F-bounded pattern without a parent field — empty array should
    // adopt the contextual element type from the heritage clause.
    let source = format!("{TREE_BTREE_INTERFACES}const bt: BTree = {{ value: 1, children: [] }};");
    let diags = get_all_diagnostics(&source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322, 0,
        "Expected no TS2322: empty array in minimal F-bounded object literal should adopt contextual type: {diags:?}"
    );
}

#[test]
fn test_ts2322_fbounded_no_parent_field_wrong_element_type_errors() {
    // When the element type is wrong, TS2322 must still fire.
    let source =
        format!("{TREE_BTREE_INTERFACES}const bt: BTree = {{ value: 1, children: [42] }};");
    let diags = get_all_diagnostics(&source);
    let ts2322 = diagnostic_count(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322 >= 1,
        "Expected TS2322: number is not assignable to BTree: {diags:?}"
    );
}

#[test]
fn test_ts2345_concrete_value_to_never_param_errors() {
    // Negative: concrete types remain non-assignable to never (the fix must not loosen this).
    let source = r#"
        declare function needsNever(x: never): void;
        needsNever(42);
    "#;
    let diags = get_all_diagnostics(source);
    let ts2345 = diagnostic_count(
        &diags,
        diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
    );
    assert!(
        ts2345 >= 1,
        "Expected TS2345 when passing number to `never` param: {diags:?}"
    );
}

#[test]
fn conditional_type_result_object_literal_reports_each_bad_property() {
    let source = r#"
interface Dog {
    type: "dog";
    breeds: "Hound" | "Shepherd";
}

type LookUp<U, T> = U extends { type: T } ? U : never;
type MyDog = LookUp<Dog, "dog">;

const wrong: MyDog = { type: "cat", breeds: "Curl" };
"#;

    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        2,
        "expected one TS2322 for each mismatched property, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type '\"cat\"' is not assignable to type '\"dog\"'")),
        "expected the discriminant property mismatch, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| {
            message.contains("Type '\"Curl\"' is not assignable to type '\"Hound\" | \"Shepherd\"'")
        }),
        "expected the union-literal property mismatch, got: {messages:#?}"
    );
}

#[test]
fn renamed_conditional_result_object_literal_reports_each_bad_property() {
    let source = r#"
interface Cat {
    kind: "cat";
    color: "black" | "white";
}

type Choose<Each, Wanted> = Each extends { kind: Wanted } ? Each : never;
type PickedCat = Choose<Cat, "cat">;

const wrong: PickedCat = { kind: "dog", color: "orange" };
"#;

    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        2,
        "expected renamed conditional result to report both property mismatches, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type '\"dog\"' is not assignable to type '\"cat\"'")),
        "expected renamed discriminant property mismatch, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| {
            message.contains("Type '\"orange\"' is not assignable to type '\"black\" | \"white\"'")
        }),
        "expected renamed union-literal property mismatch, got: {messages:#?}"
    );
}

#[test]
fn inline_conditional_result_object_literal_reports_each_bad_property() {
    let source = r#"
interface Bird {
    tag: "bird";
    wings: 2 | 4;
}

type Select<Member, Tag> = Member extends { tag: Tag } ? Member : never;

const wrong: Select<Bird, "bird"> = { tag: "fish", wings: 6 };
"#;

    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        2,
        "expected inline conditional result to report both property mismatches, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type '\"fish\"' is not assignable to type '\"bird\"'")),
        "expected inline discriminant property mismatch, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type '6' is not assignable to type '2 | 4'")),
        "expected inline numeric-union property mismatch, got: {messages:#?}"
    );
}

// =============================================================================
// Intersection source assigned to callable-interface target (issue #6202)
// =============================================================================

/// Structural rule: when an intersection containing a function member and object
/// members is assigned to a callable interface with properties, the failure message
/// must list only properties not supplied by any intersection member.
#[test]
fn intersection_function_object_satisfies_callable_interface_with_matching_props() {
    // All required call sig and properties are satisfied by the intersection.
    let source = r#"
interface CallableWithProps {
  (x: number): string;
  version: string;
}

const cwp: CallableWithProps = Object.assign(
  (x: number) => x.toString(),
  { version: "1.0" }
);
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors: fn-object intersection should satisfy callable interface with matching properties. Got: {diagnostics:#?}"
    );
}

#[test]
fn intersection_type_alias_satisfies_callable_interface_with_matching_props() {
    // Same structural rule via explicit type alias (not Object.assign).
    let source = r#"
interface CallableWithProps {
  (x: number): string;
  version: string;
}

type FnWithVersion = ((x: number) => string) & { version: string };
declare const fn1: FnWithVersion;
const cwp: CallableWithProps = fn1;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors: explicit fn-object intersection alias should satisfy callable interface. Got: {diagnostics:#?}"
    );
}

#[test]
fn intersection_function_object_missing_one_prop_reports_only_that_prop() {
    // The intersection provides `version` but not `name`.
    // The error should name only `name`, not both `version` and `name`.
    let source = r#"
interface RequiresAll {
  (x: number): string;
  version: string;
  name: string;
}

type FnWithVersionOnly = ((x: number) => string) & { version: string };
declare const cwp2: FnWithVersionOnly;
const cwp: RequiresAll = cwp2;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        !diagnostics.is_empty(),
        "Expected an error when required property `name` is missing from intersection source."
    );
    // Should NOT report `version` as missing; it is supplied by the object member.
    for diag in &diagnostics {
        let msg = &diag.message_text;
        assert!(
            !msg.contains("version"),
            "Error message should not mention `version` (it is present in the intersection): {msg}"
        );
    }
}

#[test]
fn intersection_type_alias_missing_one_prop_reports_only_that_prop() {
    // Same rule via explicit intersection alias.
    let source = r#"
interface RequiresAll {
  (x: number): string;
  version: string;
  name: string;
}

type FnWithVersion = ((x: number) => string) & { version: string };
declare const fn1: FnWithVersion;
const cwp: RequiresAll = fn1;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        !diagnostics.is_empty(),
        "Expected an error when required property `name` is missing from intersection alias."
    );
    for diag in &diagnostics {
        let msg = &diag.message_text;
        assert!(
            !msg.contains("version"),
            "Error message should not mention `version` (it is present in intersection member): {msg}"
        );
    }
}

#[test]
fn intersection_function_object_all_props_missing_reports_all() {
    // The intersection has no object member providing properties.
    // The error should list both required properties.
    let source = r#"
interface RequiresAll {
  (x: number): string;
  version: string;
  name: string;
}

type FnOnly = (x: number) => string;
declare const cwp2: FnOnly;
const cwp: RequiresAll = cwp2;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        !diagnostics.is_empty(),
        "Expected an error when all required properties are missing from intersection source."
    );
}

#[test]
fn inline_intersection_satisfies_callable_interface_multiple_props() {
    // Intersection with multiple properties covering all target requirements.
    let source = r#"
interface FullCallable {
  (x: number): string;
  name: string;
  version: number;
}

const fc: FullCallable = Object.assign(
  (x: number) => x.toString(),
  { name: "fn", version: 1 }
);
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors: fn-object intersection with all props should satisfy callable interface. Got: {diagnostics:#?}"
    );
}

#[test]
fn named_function_with_assign_satisfies_callable_interface() {
    // Named function (not arrow) assigned via Object.assign.
    let source = r#"
interface CallableWithProps {
  (x: number): string;
  version: string;
}

function myFn(x: number): string { return x.toString(); }
const cwp: CallableWithProps = Object.assign(myFn, { version: "1.0" });
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no errors: named function + Object.assign should satisfy callable interface. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2820_preserves_generic_interface_target_surface_structurally() {
    let source = r#"
interface Box<T> { kind: T; count: 1; }
declare let got: Box<"frist">;
let expected: Box<"first" | "second"> = got;
"#;

    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 spelling-suggestion diagnostic, got: {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Box<\"first\" | \"second\">"),
        "generic interface target surface should be preserved structurally, got: {message}"
    );
}

#[test]
fn ts2820_preserves_renamed_generic_alias_target_surface_structurally() {
    let source = r#"
type Wrapper<Value> = { kind: Value; count: 1 };
declare let got: Wrapper<"frist">;
let expected: Wrapper<"first" | "second"> = got;
"#;

    let messages = ts2322_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected one TS2322 spelling-suggestion diagnostic, got: {messages:#?}"
    );
    let message = &messages[0];
    assert!(
        message.contains("Wrapper<\"first\" | \"second\">"),
        "generic alias target surface should be preserved independent of parameter name, got: {message}"
    );
}

/// Regression test for issue #6800.
///
/// When an overloaded generic function is called with an inline arrow
/// callback, the first-pass overload resolution collects argument types once
/// using the union of all overload signatures as the contextual type. The
/// callback parameter type therefore picks up a reference to the sigs' shared
/// type-parameter atom (`T`). The per-overload rename then renames the sig's
/// `T` to a fresh atom, leaving the arg's `T` as a stale reference. During
/// inference, that stale reference would surface as a contravariant
/// candidate and dominate the genuine covariant candidate inferred from the
/// array value, causing the resolver to fall back to the contra-candidate
/// (the bare type parameter name) rather than the widened concrete type.
///
/// The structural rule: when every contravariant candidate is a bare
/// unconstrained type parameter (so it carries no shape requirement that
/// could be violated), the informative covariant inference must win.
#[test]
fn overload_inline_callback_does_not_leak_outer_sig_type_param() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
declare function map<T>(arr: T[], fn: (x: T) => T): T[];

const mapped = map([1, 2, 3], x => String(x));
const check: string[] = mapped;
"#;
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for generic overload with callback returning different type. Got: {diagnostics:#?}"
    );
}

/// Same as `overload_inline_callback_does_not_leak_outer_sig_type_param` but
/// with the overload order reversed to verify the fix is symmetric.
#[test]
fn overload_inline_callback_does_not_leak_outer_sig_type_param_reversed_order() {
    let source = r#"
declare function map<T>(arr: T[], fn: (x: T) => T): T[];
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];

const mapped = map([1, 2, 3], x => String(x));
const check: string[] = mapped;
"#;
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
    assert!(
        diagnostics.is_empty(),
        "Expected no errors regardless of overload declaration order. Got: {diagnostics:#?}"
    );
}

/// Renamed type-parameter variant of the bug: the fix must not depend on the
/// spelling of the sig's type parameter name. Using `A`/`B` and `C` instead of
/// `T`/`U` and `T` should produce the same result.
#[test]
fn overload_inline_callback_leak_fix_is_independent_of_type_param_name() {
    let source = r#"
declare function map<A, B>(arr: A[], fn: (x: A) => B): B[];
declare function map<C>(arr: C[], fn: (x: C) => C): C[];

const mapped = map([1, 2, 3], x => String(x));
const check: string[] = mapped;
"#;
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
    assert!(
        diagnostics.is_empty(),
        "Fix must be structural, not name-dependent. Got: {diagnostics:#?}"
    );
}

/// Negative case for the same fix: when the inline callback genuinely returns
/// the input type, the `T`-identity overload should match cleanly without
/// requiring the leak guard.
#[test]
fn overload_inline_callback_identity_overload_still_matches() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
declare function map<T>(arr: T[], fn: (x: T) => T): T[];

const arr: number[] = [1, 2, 3];
const mapped = map(arr, x => x + 1);
const check: number[] = mapped;
"#;
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", CheckerOptions::default());
    assert!(
        diagnostics.is_empty(),
        "Identity-return callback should pick either overload and yield `number[]`. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2820_preserves_generic_alias_application_inside_union_target() {
    let source = r#"
type Values<T> = T[keyof T];
type ExtractFields<Options> = Values<{
  [K in keyof Options]: Options[K] extends object ? keyof Options[K] : never;
}>;
type SetType<Options> = {
  [key: string]: any;
  target?: ExtractFields<Options>;
};
declare function test<OptionsData extends SetType<OptionsData>>(options: OptionsData): void;

test({
  target: "$test6",
  data1: { $test1: 111, $test2: null },
  data2: { $test3: {}, $test4: () => {}, $test5() {} },
});
"#;
    let msgs = ts2820_messages(source);
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one TS2820 diagnostic, got: {msgs:#?}"
    );
    let msg = &msgs[0];
    assert!(
        msg.contains("ExtractFields<"),
        "ts2820 target should preserve the ExtractFields<...> alias form, got: {msg}"
    );
    assert!(
        msg.contains("Did you mean"),
        "ts2820 should include a spelling suggestion, got: {msg}"
    );
}

#[test]
fn ts2820_preserves_generic_alias_application_inside_union_target_renamed_param() {
    // "alphx" is one character off from "alpha" to trigger the spelling suggestion.
    let source = r#"
type AllValues<U> = U[keyof U];
type PickFields<Config> = AllValues<{
  [K in keyof Config]: Config[K] extends object ? keyof Config[K] : never;
}>;
type Schema<Config> = {
  [key: string]: any;
  target?: PickFields<Config>;
};
declare function run<C extends Schema<C>>(opts: C): void;

run({
  target: "alphx",
  group1: { alpha: 1, beta: null },
  group2: { gamma: {}, delta: () => {} },
});
"#;
    let msgs = ts2820_messages(source);
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one TS2820 diagnostic with renamed params, got: {msgs:#?}"
    );
    let msg = &msgs[0];
    assert!(
        msg.contains("PickFields<"),
        "ts2820 target should preserve PickFields<...> alias form regardless of type param name, got: {msg}"
    );
}

#[test]
fn ts2820_preserves_application_union_with_null_instead_of_undefined() {
    let source = r#"
interface Container<T> { value: T; tag: 1 }
declare let src: Container<"frist">;
declare let dst: Container<"first" | "second"> | null;
dst = src;
"#;
    let all = get_all_diagnostics(source);
    let msg = all
        .iter()
        .find_map(|(code, msg)| (*code == 2322 || *code == 2820).then_some(msg))
        .unwrap_or_else(|| panic!("expected a 2322/2820 diagnostic, got: {all:#?}"));
    assert!(
        msg.contains("Container<"),
        "ts2820/ts2322 should preserve Container<...> alias when target is application | null, got: {msg}"
    );
}

#[test]
fn ts2820_union_of_plain_string_literals_uses_literal_union_form() {
    let source = r#"
declare let c: "bleu";
let x: "red" | "green" | "blue" = c;
"#;
    let all = get_all_diagnostics(source);
    let msg = all
        .iter()
        .find_map(|(code, msg)| (*code == 2322 || *code == 2820).then_some(msg))
        .unwrap_or_else(|| panic!("expected a type mismatch diagnostic, got none"));
    assert!(
        msg.contains("\"red\" | \"green\" | \"blue\""),
        "plain string literal union target should use full literal union form, got: {msg}"
    );
}
