use crate::context::CheckerOptions;
use crate::test_utils::{
    check_multi_file, check_multi_file_with_libs, check_source_diagnostics, check_source_with_libs,
    load_default_lib_files,
};
use tsz_common::common::ModuleKind;

#[test]
fn generic_promise_then_flattens_promise_return_from_callback() {
    let diags = check_source_diagnostics(
        r#"
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}

interface Promise<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): Promise<TResult1>;
}

interface Response {
    json(): Promise<{ entries: string[] }>;
}

declare function fetch(url: string): Promise<Response>;

fetch("/entries")
    .then(res => res.json())
    .then(data => {
        const entries: string[] = data.entries;
        return entries;
    });
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected Promise.then callback returning Promise<T> to infer T, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn zod_parse_async_promise_resolve_awaited_union_flattens_to_sync_return() {
    let diags = check_source_diagnostics(
        r#"
type Awaited<T> =
    T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
        F extends ((value: infer V, ...args: infer _) => any) ? Awaited<V> : never :
    T;

interface Promise<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): Promise<TResult1>;
}
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}
interface PromiseConstructor {
    resolve<T>(value: T): Promise<Awaited<T>>;
}
declare var Promise: PromiseConstructor;

type INVALID = { valid: false };
type OK<T> = { valid: true; value: T };
type SyncParseReturnType<T> = OK<T> | INVALID;
type AsyncParseReturnType<T> = Promise<SyncParseReturnType<T>>;
type ParseReturnType<T> = SyncParseReturnType<T> | AsyncParseReturnType<T>;

abstract class ZodType<Output> {
    abstract _parse(): ParseReturnType<Output>;
    _parseAsync(): AsyncParseReturnType<Output> {
        const result = this._parse();
        return Promise.resolve(result);
    }
}
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        relevant.len(),
        0,
        "Promise.resolve(ParseReturnType<T>) should flatten Awaited<T> to SyncParseReturnType<T>; got: {relevant:#?}"
    );
}

#[test]
fn object_literal_function_property_can_read_earlier_self_property() {
    let diags = check_source_diagnostics(
        r#"
const entryKeys = {
    all: ['entries'] as const,
    list: () => [...entryKeys.all, 'list'] as const
};

declare function takesKey(key: readonly ['entries', 'list']): void;
takesKey(entryKeys.list());
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345 | 7022 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected self-referencing object literal property to keep earlier property type, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn conditional_with_failed_infer_uses_false_branch() {
    let diags = check_source_diagnostics(
        r#"
interface Register {}
interface Error { message: string }
type DefaultError = Register extends { defaultError: infer TError } ? TError : Error;
declare const err: DefaultError;
declare function takesError(err: Error): void;
takesError(err);
declare function takesString(value: string): void;
takesString(err);
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        1,
        "Expected conditional infer miss to resolve false branch and reject string, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn namespace_helper_keeps_explicit_indexed_access_callable_type() {
    let diags = check_source_diagnostics(
        r#"
interface ObjectConstructor {
    keys(o: object): string[];
}
declare var Object: ObjectConstructor;

export namespace util {
    export const objectKeys: ObjectConstructor["keys"] = Object.keys;
    export const objectValues = (obj: any) => objectKeys(obj).map(key => obj[key]);
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2349 | 7006))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected namespace helper annotation to make objectKeys callable, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn switch_discriminant_can_use_const_object_member_cases() {
    let diags = check_source_diagnostics(
        r#"
const IssueCode = {
    invalid_type: "invalid_type",
    unrecognized_keys: "unrecognized_keys",
} as const;

interface InvalidTypeIssue {
    code: typeof IssueCode.invalid_type;
    received: "string" | "undefined";
}
interface UnrecognizedKeysIssue {
    code: typeof IssueCode.unrecognized_keys;
    keys: string[];
}
type Issue = InvalidTypeIssue | UnrecognizedKeysIssue;

declare function assertNever(issue: never): never;

function message(issue: Issue): string {
    switch (issue.code) {
        case IssueCode.invalid_type:
            return issue.received;
        case IssueCode.unrecognized_keys:
            const keys: string[] = issue.keys;
            return "";
        default:
            return assertNever(issue);
    }
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected const-object switch cases to narrow discriminated union, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn switch_discriminant_can_use_array_to_enum_member_cases() {
    let diags = check_source_diagnostics(
        r#"
namespace util {
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        for (const item of items) obj[item] = item;
        return obj as any;
    };

    export function assertNever(_x: never): never {
        throw new Error();
    }
}

const IssueCode = util.arrayToEnum([
    "invalid_type",
    "unrecognized_keys",
]);
type IssueCode = keyof typeof IssueCode;

interface IssueBase {
    path: (string | number)[];
    message?: string;
}
interface InvalidTypeIssue extends IssueBase {
    code: typeof IssueCode.invalid_type;
    received: "string" | "undefined";
}
interface UnrecognizedKeysIssue extends IssueBase {
    code: typeof IssueCode.unrecognized_keys;
    keys: string[];
}
type Issue = InvalidTypeIssue | UnrecognizedKeysIssue;

function message(issue: Issue): string {
    let message: string;
    switch (issue.code) {
        case IssueCode.invalid_type:
            if (issue.received === "undefined") {
                message = "Required";
            } else {
                message = issue.received;
            }
            break;
        case IssueCode.unrecognized_keys:
            const keys: string[] = issue.keys;
            message = "";
            break;
        default:
            return util.assertNever(issue);
    }
    return message;
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected arrayToEnum switch cases to narrow discriminated union, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn keyof_array_to_enum_includes_every_string_literal_key() {
    let diags = check_source_diagnostics(
        r#"
namespace util {
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        for (const item of items) obj[item] = item;
        return obj as any;
    };
}

export const ParsedType = util.arrayToEnum([
    "string",
    "undefined",
    "object",
]);

export type ParsedType = keyof typeof ParsedType;

declare const received: ParsedType;
declare function takeParsed(value: ParsedType): ParsedType;

const direct: ParsedType = ParsedType.undefined;
const viaCall: ParsedType = takeParsed(ParsedType.string);

if (received === "undefined") {
    const value: "undefined" = received;
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2345 | 2367))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected keyof arrayToEnum to include all literal keys, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_data_preserves_union_properties_through_omit_and_spread() {
    let diags = check_source_diagnostics(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

namespace util {
    export type OmitKeys<T, K extends string> = Pick<T, Exclude<keyof T, K>>;
}

interface IssueBase {
    path: (string | number)[];
    message?: string;
}
interface InvalidTypeIssue extends IssueBase {
    code: "invalid_type";
    expected: "string";
    received: "undefined" | "string";
}
interface CustomIssue extends IssueBase {
    code: "custom";
    params?: { [k: string]: any };
}

type IssueOptionalMessage = InvalidTypeIssue | CustomIssue;
type StripPath<T extends object> = T extends any ? util.OmitKeys<T, "path"> : never;
type IssueData = StripPath<IssueOptionalMessage> & { path?: (string | number)[] };

declare const issueData: IssueData;
const fullPath: (string | number)[] = [];
const fullIssue = {
    ...issueData,
    path: fullPath,
};

declare function map(issue: IssueOptionalMessage): void;
map(fullIssue);
const msg = issueData.message || "";
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345 | 2322))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected Zod IssueData to preserve union properties through OmitKeys and spread, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_5030_defaults_path_with_logical_or_array_literal() {
    let diags = check_source_diagnostics(
        r#"
type ParseParams = {
    path: (string | number)[];
    data: unknown;
    errorMap: unknown;
    async: boolean;
};

type Partial<T> = { [P in keyof T]?: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

type ParseParamsNoData = Omit<ParseParams, "data">;
type ParsePathComponent = string | number;

declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function test(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
    pathFromArray(params.path ?? []);
}
"#,
    );

    assert!(
        diags.is_empty(),
        "Expected `params.path || []` to keep path as `(string | number)[]` in argument position, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_5030_defaults_path_with_lib_utility_aliases() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");

    let diags = check_source_with_libs(
        r#"
type ParseParams = {
    path: (string | number)[];
    data: unknown;
    errorMap: unknown;
    async: boolean;
};

type ParseParamsNoData = Omit<ParseParams, "data">;
type ParsePathComponent = string | number;

declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function test(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diags.is_empty(),
        "Expected lib-backed `Partial<Omit<...>>` to contextually type `params.path || []` and `params.path ?? []`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_cross_file_object_literal_reduces_partial_omit_property_read() {
    let diags = check_multi_file(
        &[
            (
                "zod-error-map-min.ts",
                "export type ZodErrorMap = () => { message: string };\n",
            ),
            (
                "zod-parse-util-min.ts",
                r#"
import { ZodErrorMap } from "./zod-error-map-min";

export type Partial<T> = { [P in keyof T]?: T[P] };
export type Pick<T, K extends keyof T> = { [P in K]: T[P] };
export type Exclude<T, U> = T extends U ? never : T;
export type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

export type ParseParams = {
    data: unknown;
    errorMap: ZodErrorMap;
};

export type ParseParamsNoData = Omit<ParseParams, "data">;
"#,
            ),
            (
                "zod-types-min.ts",
                r#"
import { ParseParamsNoData, Partial } from "./zod-parse-util-min";
import { ZodErrorMap } from "./zod-error-map-min";

type ZodObjectContextTarget = { errorMap?: ZodErrorMap };
declare function useContext(def: ZodObjectContextTarget): void;

function createRootContext(params: Partial<ParseParamsNoData>) {
    useContext({
        errorMap: params.errorMap,
    });
}
"#,
            ),
        ],
        "zod-types-min.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diags.is_empty(),
        "Expected cross-file object literal context to reduce `Partial<Omit<...>>` property reads, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_cross_file_lib_partial_omit_preserves_imported_path_property() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");

    let diags = check_multi_file_with_libs(
        &[
            (
                "zod-error.ts",
                r#"
import { ZodParsedType } from "./parse-util";

export type ZodIssue = { path: (string | number)[]; parsed: ZodParsedType };
export type ZodErrorMap = (...args: any[]) => { message: string };
"#,
            ),
            (
                "parse-util.ts",
                r#"
import { ZodErrorMap } from "./zod-error";

export const ZodParsedType = {
    string: "string",
    object: "object",
} as const;
export type ZodParsedType = keyof typeof ZodParsedType;

export type ParseParams = {
    path: (string | number)[];
    errorMap: ZodErrorMap;
    async: boolean;
};

export type ParseParamsNoData = Omit<ParseParams, "data">;
"#,
            ),
            (
                "types.ts",
                r#"
import { ParseParamsNoData } from "./parse-util";

type ParsePathComponent = string | number;
declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function createRootContext(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
    pathFromArray(params.path ?? []);
}
"#,
            ),
        ],
        "types.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diags.is_empty(),
        "Expected lib-backed cross-file `Partial<Omit<...>>` to preserve `path`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_object_literal_optional_indexed_access_property_accepts_boolean() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");

    let diags = check_source_with_libs(
        r#"
type ParseParams = {
    data: unknown;
    async: boolean;
};

type ParseParamsNoData = Omit<ParseParams, "data">;
type Target = { async?: ParseParamsNoData["async"] };

declare function createRootContext(params: Target): void;

createRootContext({ async: false });
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diags.is_empty(),
        "Expected boolean to be assignable to optional Omit<...>[\"async\"] property, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_reexport_cycle_keeps_in_progress_class_method_placeholders() {
    let diags = check_multi_file(
        &[
            (
                "src/index.ts",
                r#"
export * from "./external";
"#,
            ),
            (
                "src/external.ts",
                r#"
export * from "./types";
"#,
            ),
            (
                "src/helpers/partialUtil.ts",
                r#"
import type {
  ZodArray,
  ZodNullable,
  ZodObject,
  ZodOptional,
  ZodTuple,
  ZodTupleItems,
  ZodTypeAny,
} from "../index";

export namespace partialUtil {
  export type DeepPartial<T extends ZodTypeAny> = T extends ZodObject<
    infer Shape,
    infer Params,
    infer Catchall
  >
    ? ZodObject<
        { [K in keyof Shape]: ZodOptional<DeepPartial<Shape[K]>> },
        Params,
        Catchall
      >
    : T extends ZodArray<infer Type>
    ? ZodArray<DeepPartial<Type>>
    : T extends ZodOptional<infer Type>
    ? ZodOptional<DeepPartial<Type>>
    : T extends ZodNullable<infer Type>
    ? ZodNullable<DeepPartial<Type>>
    : T extends ZodTuple<infer Items>
    ? {
        [K in keyof Items]: Items[K] extends ZodTypeAny
          ? DeepPartial<Items[K]>
          : never;
      } extends infer PartialItems
      ? PartialItems extends ZodTupleItems
        ? ZodTuple<PartialItems>
        : never
      : never
    : T;
}
"#,
            ),
            (
                "src/types.ts",
                r#"
import { partialUtil } from "./helpers/partialUtil";

type ParseReturnType<T> = T;
type SyncParseReturnType<T> = T;
type AsyncParseReturnType<T> = Promise<T>;
type Partial<T> = { [K in keyof T]?: T[K] };
interface Promise<T> {}
declare var Promise: {
  resolve<T>(value: T): Promise<T>;
};
type ZodParsedType = "string";
interface ParseContext {}
interface ParseParamsNoData {
  async?: boolean;
}
interface ZodTypeDef {}

export abstract class ZodType<
  Output,
  Def extends ZodTypeDef = ZodTypeDef,
  Input = Output
> {
  readonly _type!: Output;
  readonly _output!: Output;
  readonly _input!: Input;
  readonly _def!: Def;

  abstract _parse(
    _ctx: ParseContext,
    _data: unknown,
    _parsedType: ZodParsedType
  ): ParseReturnType<Output>;

  _parseSync(
    _ctx: ParseContext,
    _data: unknown,
    _parsedType: ZodParsedType
  ): SyncParseReturnType<Output> {
    return this._parse(_ctx, _data, _parsedType);
  }

  _parseAsync(
    _ctx: ParseContext,
    _data: unknown,
    _parsedType: ZodParsedType
  ): AsyncParseReturnType<Output> {
    return Promise.resolve(this._parse(_ctx, _data, _parsedType));
  }

  safeParse(_data: unknown, _params?: Partial<ParseParamsNoData>): { success: true; data: Output } {
    return { success: true, data: this._parseSync({}, _data, "string") };
  }
}

export type ZodTypeAny = ZodType<any, any, any>;
export type ZodTupleItems = [ZodTypeAny, ...ZodTypeAny[]];
export class ZodObject<Shape, Params, Catchall> extends ZodType<Shape> {
  _parse(_ctx: ParseContext, _data: unknown, _parsedType: ZodParsedType): Shape {
    return {} as Shape;
  }
}
export class ZodArray<Type extends ZodTypeAny> extends ZodType<Type[]> {
  _parse(_ctx: ParseContext, _data: unknown, _parsedType: ZodParsedType): Type[] {
    return [];
  }
}
export class ZodOptional<Type extends ZodTypeAny> extends ZodType<Type | undefined> {
  _parse(_ctx: ParseContext, _data: unknown, _parsedType: ZodParsedType): Type | undefined {
    return undefined;
  }
}
export class ZodNullable<Type extends ZodTypeAny> extends ZodType<Type | null> {
  _parse(_ctx: ParseContext, _data: unknown, _parsedType: ZodParsedType): Type | null {
    return null;
  }
}
export class ZodTuple<Items extends ZodTupleItems> extends ZodType<Items> {
  _parse(_ctx: ParseContext, _data: unknown, _parsedType: ZodParsedType): Items {
    return [] as unknown as Items;
  }
}

type ForceCycle = partialUtil.DeepPartial<ZodTypeAny>;
"#,
            ),
        ],
        "src/types.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    let missing_this_methods: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.code == 2339
                && (d.message_text.contains("Property '_parse'")
                    || d.message_text.contains("Property '_parseSync'")
                    || d.message_text.contains("Property 'safeParse'"))
        })
        .collect();
    assert!(
        missing_this_methods.is_empty(),
        "Expected re-entrant ZodType class construction to keep method placeholders on `this`, got: {:?}",
        missing_this_methods
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_error_field_initializer_this_keeps_accessors_and_base_members() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");

    let diags = check_source_with_libs(
        r#"
type ZodIssue = { message: string };
type ZodFormattedError<T> = { _errors: string[]; value?: T };

export class ZodError<T = any> extends Error {
  issues: ZodIssue[] = [];

  get errors() {
    return this.issues;
  }

  constructor(issues: ZodIssue[]) {
    super();
    this.name = "ZodError";
    this.issues = issues;
  }

  format = (): ZodFormattedError<T> => {
    const fieldErrors: ZodFormattedError<T> = { _errors: [] };
    const processError = (error: ZodError) => {
      error.errors;
      error.message;
      error.isEmpty;
    };
    processError(this);
    return fieldErrors;
  };

  toString() {
    return this.message;
  }

  get message() {
    return JSON.stringify(this.issues, null, 2);
  }

  get isEmpty(): boolean {
    return this.issues.length === 0;
  }

  addIssue = (sub: ZodIssue) => {
    this.issues = [...this.issues, sub];
  };
}

"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    let structural_this_errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.code == 2345
                && (d.message_text.contains("name")
                    || d.message_text.contains("errors")
                    || d.message_text.contains("message")
                    || d.message_text.contains("isEmpty"))
        })
        .collect();
    assert!(
        structural_this_errors.is_empty(),
        "Expected class field initializer `this` to include accessor and inherited Error members, got: {:?}",
        structural_this_errors
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_type_field_initializer_this_keeps_instance_method_alias() {
    let diags = crate::test_utils::check_with_options(
        r#"
type ZodTypeDef = {};
type ZodError<T = unknown> = Error & { input?: T };
type ZodTypeAny = ZodType<any, any, any>;

export abstract class ZodType<
  Output = any,
  Def extends ZodTypeDef = ZodTypeDef,
  Input = Output
> {
  async safeParseAsync(data: unknown): Promise<
    { success: true; data: Output } | { success: false; error: Error }
  > {
    return Promise.resolve({ success: true, data: data as Output });
  }

  spa = this.safeParseAsync;
}

declare const z: ZodType<string>;
z.spa;
"#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2532 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected class field initializer `this` to resolve to the instance type, got: {relevant:#?}"
    );
}
