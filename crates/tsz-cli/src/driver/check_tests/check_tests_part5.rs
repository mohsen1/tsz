fn mapped_type_indexed_access_constraint_repro() -> &'static str {
    r#"type Identity<T> = { [K in keyof T]: T[K] };

type M0 = { a: 1, b: 2 };

type M1 = { [K in keyof Partial<M0>]: M0[K] };

type M2 = { [K in keyof Required<M1>]: M1[K] };

type M3 = { [K in keyof Identity<Partial<M0>>]: M0[K] };

function foo<K extends keyof M0>(m1: M1[K], m2: M2[K], m3: M3[K]) {
    m1.toString();
    m1?.toString();
    m2.toString();
    m2?.toString();
    m3.toString();
    m3?.toString();
}

type Obj = {
    a: 1,
    b: 2
};

const mapped: { [K in keyof Partial<Obj>]: Obj[K] } = {};

const resolveMapped = <K extends keyof typeof mapped>(key: K) => mapped[key].toString();

const arr = ["foo", "12", 42] as const;

type Mappings = { foo: boolean, "12": number, 42: string };

type MapperArgs<K extends (typeof arr)[number]> = {
    v: K,
    i: number
};

type SetOptional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type PartMappings = SetOptional<Mappings, "foo">;

const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = {
    foo: ({ v, i }) => v.length + i > 4,
    "12": ({ v, i }) => Number(v) + i,
    42: ({ v, i }) => `${v}${i}`,
};

const resolveMapper1 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key](o);

const resolveMapper2 = <K extends keyof typeof mapper>(
    key: K, o: MapperArgs<K>) => mapper[key]?.(o);
"#
}

#[test]
fn merged_class_interface_retains_local_and_cross_file_construct_groups() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/class-interface-first.ts",
            r#"
interface LocalConstructResult { selected: "local" }
class MixedConstruct {}
interface MixedConstruct {
    new (value: string): LocalConstructResult;
}
class InheritedMixedConstruct {}
"#,
        ),
        (
            "/project/class-interface-second.ts",
            r#"
interface ForeignConstructResult { selected: "foreign" }
interface ForeignInheritedResult { selected: "inherited" }
interface MixedConstruct {
    new (value: number): ForeignConstructResult;
}
interface ForeignConstructBase {
    new (value: boolean): ForeignInheritedResult;
    inheritedForeign: "base";
}
interface InheritedMixedConstruct extends ForeignConstructBase {}
declare const MixedCtor: MixedConstruct;
const localResult: LocalConstructResult = new MixedCtor("value");
const foreignResult: ForeignConstructResult = new MixedCtor(1);
declare const mixedValue: MixedConstruct;
const foreignProperty: "present" = mixedValue.foreignProperty;
declare const InheritedCtor: InheritedMixedConstruct;
const inheritedResult: ForeignInheritedResult = new InheritedCtor(true);
declare const inheritedValue: InheritedMixedConstruct;
const inheritedForeign: "base" = inheritedValue.inheritedForeign;
"#,
        ),
        (
            "/project/class-interface-property.ts",
            r#"
interface MixedConstruct {
    foreignProperty: "present";
}
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "production symbol merging must retain constructor groups from every declaration arena; got: {diagnostics:?}"
    );
}

#[test]
fn module_augmentation_construct_groups_follow_tsc_candidate_order() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/construct-aug-home.ts",
            r#"
export interface HomeResult { selected: "home" }
export interface C {
    prototype: { home: true };
    new (value: string): HomeResult;
}
"#,
        ),
        (
            "/project/construct-aug-one.ts",
            r#"
import "./construct-aug-home";
declare module "./construct-aug-home" {
    interface C {
        new (value: string): { selected: "aug-one" };
        new (value: "pick"): { selected: "lit-one" };
    }
}
"#,
        ),
        (
            "/project/construct-aug-two.ts",
            r#"
import "./construct-aug-home";
declare module "./construct-aug-home" {
    interface C {
        new (value: string): { selected: "aug-two" };
        new (value: "pick"): { selected: "lit-two" };
    }
}
import type { C } from "./construct-aug-home";
declare const localCtor: C;
const localRegular: { selected: "aug-two" } = new localCtor("other");
const localLiteral: { selected: "lit-one" } = new localCtor("pick");
"#,
        ),
        (
            "/project/construct-aug-consumer.ts",
            r#"
import type { C } from "./construct-aug-home";
declare const Ctor: C;
const prototype: { home: true } = Ctor.prototype;
const regular: { selected: "aug-two" } = new Ctor("other");
const literal: { selected: "lit-one" } = new Ctor("pick");
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "module augmentation construct groups must preserve raw order for solver reordering; got: {diagnostics:?}"
    );
}

#[test]
fn class_module_augmentation_construct_signature_stays_on_instance_side() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/class-construct-home.ts",
            r#"
export class ConstructableClass {
    constructor(value: number) { void value; }
}
"#,
        ),
        (
            "/project/class-construct-augmentation.ts",
            r#"
import "./class-construct-home";
declare module "./class-construct-home" {
    interface ConstructableClass {
        new (value: string): { selected: "instance" };
    }
}
"#,
        ),
        (
            "/project/class-construct-consumer.ts",
            r#"
import { ConstructableClass } from "./class-construct-home";
new ConstructableClass("not a number");
declare const instance: ConstructableClass;
const selected: { selected: "instance" } = new instance("value");
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2345],
        "the augmentation construct signature belongs to the class instance, while the class value retains its declared constructor; got: {diagnostics:?}"
    );
}

#[test]
fn class_module_augmentation_keeps_instance_and_static_surfaces_disjoint() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/class-surface-home.ts",
            r#"
export class SplitSurface {}
"#,
        ),
        (
            "/project/class-surface-augmentation.ts",
            r#"
import "./class-surface-home";
declare module "./class-surface-home" {
    interface SplitSurface {
        instanceOnly: "instance";
        self: SplitSurface;
    }
    namespace SplitSurface {
        export const staticOnly: "static";
    }
}
"#,
        ),
        (
            "/project/class-surface-consumer.ts",
            r#"
import { SplitSurface } from "./class-surface-home";
declare const instance: SplitSurface;
const instanceValue: "instance" = instance.instanceOnly;
const staticValue: "static" = SplitSurface.staticOnly;
SplitSurface.instanceOnly;
instance.staticOnly;
const nestedInstanceValue: "instance" = instance.self.instanceOnly;
instance.self.staticOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2339, 2339],
        "interface members belong only to the class instance and namespace values only to the static class value; got: {diagnostics:?}"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.message_text.contains("instanceOnly"))
            .count(),
        1,
        "the augmentation-local self reference must retain the instance body rather than being republished as `typeof SplitSurface`: {diagnostics:?}"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diag| diag.message_text.contains("staticOnly"))
            .count(),
        2,
        "static namespace values must be rejected on both the direct and self-referenced instance: {diagnostics:?}"
    );
}

#[test]
fn function_module_augmentation_keeps_type_and_value_surfaces_disjoint() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/function-surface-home.ts",
            r#"
export function SplitCallable(value: string): string { return value; }
export interface SplitCallable {}
"#,
        ),
        (
            "/project/function-surface-augmentation.ts",
            r#"
import "./function-surface-home";
declare module "./function-surface-home" {
    interface SplitCallable {
        instanceOnly: "instance";
    }
    namespace SplitCallable {
        export const staticOnly: "static";
    }
}
"#,
        ),
        (
            "/project/function-surface-consumer.ts",
            r#"
import { SplitCallable } from "./function-surface-home";
const callResult: string = SplitCallable("value");
const staticValue: "static" = SplitCallable.staticOnly;
SplitCallable.instanceOnly;
declare const typedValue: import("./function-surface-home").SplitCallable;
const instanceValue: "instance" = typedValue.instanceOnly;
typedValue.staticOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2339],
        "a function value receives only namespace values while its same-name interface receives only type-side members; got: {diagnostics:?}"
    );
    assert!(
        diagnostics[0].message_text.contains("instanceOnly")
            && diagnostics[1].message_text.contains("staticOnly"),
        "the rejected cross-side properties must match the source order: {diagnostics:?}"
    );
}

#[test]
fn callable_interface_merged_with_function_keeps_signatures_and_values_disjoint() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/callable-surface-home.ts",
            r#"
export function CallableSplit(value: number): number;
export function CallableSplit(value: boolean): boolean;
export function CallableSplit(value: number | boolean): number | boolean { return value; }
export interface CallableSplit {
    (value: "base-type"): "base-type";
}
"#,
        ),
        (
            "/project/callable-surface-augmentation.ts",
            r#"
import "./callable-surface-home";
declare module "./callable-surface-home" {
    interface CallableSplit {
        (value: "augmented-type"): "augmented-type";
        instanceOnly: "instance";
    }
    namespace CallableSplit {
        export const staticOnly: "static";
    }
}
"#,
        ),
        (
            "/project/callable-surface-consumer.ts",
            r#"
import { CallableSplit } from "./callable-surface-home";
import * as callableHome from "./callable-surface-home";
import callableRequired = require("./callable-surface-home");
const numberResult: number = CallableSplit(1);
const booleanResult: boolean = CallableSplit(true);
const staticValue: "static" = CallableSplit.staticOnly;
CallableSplit.instanceOnly;
CallableSplit("augmented-type");
declare const typedValue: import("./callable-surface-home").CallableSplit;
const baseResult: "base-type" = typedValue("base-type");
const augmentedResult: "augmented-type" = typedValue("augmented-type");
const instanceValue: "instance" = typedValue.instanceOnly;
typedValue.staticOnly;
const namespaceNumber: number = callableHome.CallableSplit(1);
const namespaceBoolean: boolean = callableHome.CallableSplit(true);
const namespaceStatic: "static" = callableHome.CallableSplit.staticOnly;
callableHome.CallableSplit.instanceOnly;
callableHome.CallableSplit("augmented-type");
const requiredNumber: number = callableRequired.CallableSplit(1);
const requiredStatic: "static" = callableRequired.CallableSplit.staticOnly;
callableRequired.CallableSplit.instanceOnly;
callableRequired.CallableSplit("augmented-type");
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2769, 2339, 2339, 2769, 2339, 2769],
        "the function value must keep its complete runtime overload set and namespace values while the callable interface receives only type-side augmentation members; got: {diagnostics:?}"
    );
    assert!(
        diagnostics[0].message_text.contains("instanceOnly")
            && diagnostics[2].message_text.contains("staticOnly")
            && diagnostics[3].message_text.contains("instanceOnly"),
        "the rejected properties must stay on their opposite declaration spaces: {diagnostics:?}"
    );
}

#[test]
fn function_only_export_keeps_augmentation_type_and_value_surfaces_disjoint() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/function-only-home.ts",
            r#"
export function RuntimeOnly(value: number): number { return value; }
"#,
        ),
        (
            "/project/function-only-augmentation.ts",
            r#"
import "./function-only-home";
declare module "./function-only-home" {
    interface RuntimeOnly {
        (value: "type-only"): "type-only";
        instanceOnly: true;
    }
    namespace RuntimeOnly {
        export const staticOnly: "static";
    }
}
"#,
        ),
        (
            "/project/function-only-consumer.ts",
            r#"
import { RuntimeOnly } from "./function-only-home";
const result: number = RuntimeOnly(1);
const staticValue: "static" = RuntimeOnly.staticOnly;
RuntimeOnly.instanceOnly;
RuntimeOnly("type-only");
declare const typedValue: import("./function-only-home").RuntimeOnly;
const typedCall: "type-only" = typedValue("type-only");
const typedProperty: true = typedValue.instanceOnly;
typedValue.staticOnly;
typedValue(1);
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2345, 2339, 2345],
        "augmentation-only type and value meanings must start from disjoint identity-neutral surfaces; got: {diagnostics:?}"
    );
    assert!(diagnostics[0].message_text.contains("instanceOnly"));
    assert!(diagnostics[2].message_text.contains("staticOnly"));
}

#[test]
fn interface_only_export_gains_a_disjoint_namespace_value_surface() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/interface-only-home.ts",
            r#"
export interface TypeOnlyExport { instanceOnly: "instance" }
"#,
        ),
        (
            "/project/interface-only-augmentation.ts",
            r#"
import "./interface-only-home";
declare module "./interface-only-home" {
    namespace TypeOnlyExport {
        export const staticOnly: "static";
    }
}
"#,
        ),
        (
            "/project/interface-only-consumer.ts",
            r#"
import { TypeOnlyExport } from "./interface-only-home";
import * as home from "./interface-only-home";
import requiredHome = require("./interface-only-home");
const staticValue: "static" = TypeOnlyExport.staticOnly;
TypeOnlyExport.instanceOnly;
declare const typedValue: TypeOnlyExport;
const instanceValue: "instance" = typedValue.instanceOnly;
typedValue.staticOnly;
const namespaceStatic: "static" = home.TypeOnlyExport.staticOnly;
home.TypeOnlyExport.instanceOnly;
const requiredStatic: "static" = requiredHome.TypeOnlyExport.staticOnly;
requiredHome.TypeOnlyExport.instanceOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2339, 2339, 2339],
        "an augmentation-only namespace value must not inherit the interface instance surface; got: {diagnostics:?}"
    );
    assert!(diagnostics[0].message_text.contains("instanceOnly"));
    assert!(diagnostics[1].message_text.contains("staticOnly"));
    assert!(diagnostics[2].message_text.contains("instanceOnly"));
    assert!(diagnostics[3].message_text.contains("instanceOnly"));
}

#[test]
fn native_type_only_namespace_exact_recovers_an_augmented_enum_value() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/native-type-namespace-home.ts",
            r#"
export namespace NativeTypes {
    export interface Shape { kind: "shape" }
}
"#,
        ),
        (
            "/project/native-type-namespace-augmentation.d.ts",
            r#"
import "./native-type-namespace-home";
declare module "./native-type-namespace-home" {
    export namespace NativeTypes {
        export enum RuntimeEnum { Member }
    }
}
"#,
        ),
        (
            "/project/native-type-namespace-consumer.ts",
            r#"
import { NativeTypes } from "./native-type-namespace-home";
import type {
    NativeTypes as ExplicitTypeOnlyNativeTypes,
} from "./native-type-namespace-home";

const regularMember: number =
    NativeTypes.RuntimeEnum.Member;
const typeOnlyMemberIsNotString: string =
    ExplicitTypeOnlyNativeTypes.RuntimeEnum.Member;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![1361, 2322],
        "a native uninstantiated namespace must gain the augmentation-owned enum value, while an explicit type-only import diagnoses and continues with the exact enum member type; got: {diagnostics:?}"
    );
}

#[test]
fn augmentation_introduced_function_keeps_cross_file_overloads_and_namespace_values() {
    let diagnostics = collect_test_diagnostics(&[
        ("/project/new-runtime-home.ts", "export {};"),
        (
            "/project/new-runtime-string.ts",
            r#"
import "./new-runtime-home";
declare module "./new-runtime-home" {
    export function AddedRuntime(value: string): "string";
}
"#,
        ),
        (
            "/project/new-runtime-number.d.ts",
            r#"
import "./new-runtime-home";
declare module "./new-runtime-home" {
    export function AddedRuntime(value: number): "number";
    export namespace AddedRuntime {
        export const staticOnly: "static";
    }
    export enum AddedEnum { Member }
    export const enum AddedConstEnum { Member }
    export namespace Dotted.Inner {
        export const leaf: "leaf";
    }
    export namespace RuntimeOnlyNamespace {
        type TypeOnly = string;
        export { TypeOnly };
    }
    export interface TypeOnlyAugmentation {
        instanceOnly: true;
    }
}
"#,
        ),
        (
            "/project/new-runtime-consumer.ts",
            r#"
import { AddedRuntime } from "./new-runtime-home";
import * as home from "./new-runtime-home";
import requiredHome = require("./new-runtime-home");
const stringResult: "string" = AddedRuntime("value");
const numberResult: "number" = AddedRuntime(1);
const staticValue: "static" = AddedRuntime.staticOnly;
const namespaceString: "string" = home.AddedRuntime("value");
const namespaceNumber: "number" = home.AddedRuntime(1);
const namespaceStatic: "static" = home.AddedRuntime.staticOnly;
const requiredString: "string" = requiredHome.AddedRuntime("value");
const requiredNumber: "number" = requiredHome.AddedRuntime(1);
const requiredStatic: "static" = requiredHome.AddedRuntime.staticOnly;
const enumMember: home.AddedEnum = home.AddedEnum.Member;
const constEnumMember: home.AddedConstEnum = home.AddedConstEnum.Member;
const dottedLeaf: "leaf" = home.Dotted.Inner.leaf;
AddedRuntime(true);
home.TypeOnlyAugmentation;
requiredHome.TypeOnlyAugmentation;
const enumObjectIsNotAnEnumMember: home.AddedEnum = home.AddedEnum;
const constEnumObject = home.AddedConstEnum;
home.RuntimeOnlyNamespace;
requiredHome.RuntimeOnlyNamespace;
type RuntimeOnlyNamespaceValue =
    typeof import("./new-runtime-home").RuntimeOnlyNamespace;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2769, 2339, 2339, 2322, 2475, 2339, 2339, 2694],
        "cross-file runtime overload groups and companion namespace values must aggregate exactly, while a type-only augmentation name stays absent from the namespace value; got: {diagnostics:?}"
    );
}

#[test]
fn non_identifier_type_query_diagnostics_follow_normal_source_order() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/query-order-home.ts",
            "export const available = 1;",
        ),
        (
            "/project/query-order-consumer.ts",
            r#"
import { available } from "./query-order-home";
available.before;
type Missing = typeof import("./query-order-home").absent;
available.after;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2339, 2694, 2339],
        "a non-identifier typeof query must be diagnosed where it appears instead of during type-environment prewarming; got: {diagnostics:?}"
    );
}

#[test]
fn native_function_augmentation_overloads_reach_every_value_namespace() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/native-function-home.d.ts",
            r#"
export function nativeFunction(value: string): "string";
export interface NativeInterface { instanceOnly: true }
export type NativeAlias = { aliasOnly: true };
export interface FunctionOnly { instanceOnly: true }
export type ConstOnly = { aliasOnly: true };
"#,
        ),
        (
            "/project/native-function-augmentation.d.ts",
            r#"
import "./native-function-home";
declare module "./native-function-home" {
    export function nativeFunction(value: number): "number";
    export namespace nativeFunction {
        export const meta: "function-meta";
    }
    export function AddedRuntime(value: boolean): "added";
    export namespace AddedRuntime {
        export const meta: "added-meta";
    }
    export namespace NativeInterface {
        export const meta: "interface-meta";
    }
    export namespace NativeAlias {
        export const meta: "alias-meta";
    }
    export function FunctionOnly(): "function-only";
    export const ConstOnly: { readonly meta: "const-meta" };
}
"#,
        ),
        (
            "/project/native-function-consumer.ts",
            r#"
import {
    ConstOnly,
    FunctionOnly,
    nativeFunction,
} from "./native-function-home";
import * as namespaceHome from "./native-function-home";
import requiredHome = require("./native-function-home");

const namedString: "string" = nativeFunction("value");
const namedNumber: "number" = nativeFunction(1);
const namedMeta: "function-meta" = nativeFunction.meta;
const namespaceString: "string" = namespaceHome.nativeFunction("value");
const namespaceNumber: "number" = namespaceHome.nativeFunction(1);
const namespaceMeta: "function-meta" = namespaceHome.nativeFunction.meta;
const requiredString: "string" = requiredHome.nativeFunction("value");
const requiredNumber: "number" = requiredHome.nativeFunction(1);
const requiredMeta: "function-meta" = requiredHome.nativeFunction.meta;

declare const typeofFunction: typeof import("./native-function-home").nativeFunction;
const typeofString: "string" = typeofFunction("value");
const typeofNumber: "number" = typeofFunction(1);
const typeofMeta: "function-meta" = typeofFunction.meta;
declare const addedRuntime: typeof import("./native-function-home").AddedRuntime;
const addedResult: "added" = addedRuntime(true);
const addedMeta: "added-meta" = addedRuntime.meta;
declare const nativeInterface:
    typeof import("./native-function-home").NativeInterface;
const interfaceMeta: "interface-meta" = nativeInterface.meta;
declare const nativeAlias: typeof import("./native-function-home").NativeAlias;
const aliasMeta: "alias-meta" = nativeAlias.meta;
const functionOnlyResult: "function-only" = FunctionOnly();
const constOnlyMeta: "const-meta" = ConstOnly.meta;
const constOnlyType: ConstOnly = { aliasOnly: true };

const namedWrongReturn: "string" = nativeFunction(1);
nativeFunction(true);
const namespaceWrongReturn: "string" = namespaceHome.nativeFunction(1);
namespaceHome.nativeFunction(true);
const requiredWrongReturn: "string" = requiredHome.nativeFunction(1);
requiredHome.nativeFunction(true);
const typeofWrongReturn: "string" = typeofFunction(1);
typeofFunction(true);
FunctionOnly.instanceOnly;
ConstOnly.aliasOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2322, 2769, 2322, 2769, 2322, 2769, 2322, 2769, 2339, 2339],
        "native runtime overloads and namespace companions must flow through named, star, require, and `typeof import()` value paths without leaking type-side call signatures; got: {diagnostics:?}"
    );
}

const NAMED_AUGMENTATION_HOME: &str = r#"
export interface FunctionOnly { instanceOnly: true }
export type ConstOnly = { aliasOnly: true };
export interface PlainType { base: "plain" }
export interface GenericType<T> { value: T }
"#;

const NAMED_AUGMENTATION_DECLARATIONS: &str = r#"
import "./named-augmentation-home";
declare module "./named-augmentation-home" {
    export function FunctionOnly(): "function-only";
    export const ConstOnly: { readonly meta: "const-meta" };
    interface PlainType { augmented: "plain-augmented" }
    interface GenericType<T> { augmentedValue: T }
}
"#;

fn named_augmentation_files(consumer: &str) -> [(&str, &str); 3] {
    [
        (
            "/project/named-augmentation-home.ts",
            NAMED_AUGMENTATION_HOME,
        ),
        (
            "/project/named-augmentation-declarations.ts",
            NAMED_AUGMENTATION_DECLARATIONS,
        ),
        ("/project/named-augmentation-consumer.ts", consumer),
    ]
}

#[test]
fn renamed_named_imports_receive_augmentation_runtime_values() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import {
    ConstOnly as RenamedConst,
    FunctionOnly as RenamedFunction,
} from "./named-augmentation-home";
const result: "function-only" = RenamedFunction();
const meta: "const-meta" = RenamedConst.meta;
RenamedFunction.instanceOnly;
RenamedConst.aliasOnly;
"#,
    ));
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2339, 2339],
        "renaming a regular named import must retain exact augmentation runtime values without leaking the original type surface; got: {diagnostics:?}"
    );
}

#[test]
fn inner_pure_type_shadow_keeps_outer_augmented_import_value_visible() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import {
    FunctionOnly as RenamedRuntime,
} from "./named-augmentation-home";
{
    type RenamedRuntime = { localOnly: "local" };
    const result: "function-only" = RenamedRuntime();
    const local: RenamedRuntime = { localOnly: "local" };
    local.instanceOnly;
}
"#,
    ));
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2339],
        "a nearer pure-type declaration shadows only the type meaning; the outer imported augmentation value must remain callable: {diagnostics:?}"
    );
    assert!(diagnostics[0].message_text.contains("instanceOnly"));
}

#[test]
fn renamed_type_only_imports_report_value_use_without_callability_noise() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import type {
    ConstOnly as ExplicitConst,
    FunctionOnly as ExplicitFunction,
} from "./named-augmentation-home";
import {
    type ConstOnly as InlineConst,
    type FunctionOnly as InlineFunction,
} from "./named-augmentation-home";
ExplicitFunction();
ExplicitConst.meta;
InlineFunction();
InlineConst.meta;
ExplicitFunction.instanceOnly;
ExplicitConst.aliasOnly;
InlineFunction.instanceOnly;
InlineConst.aliasOnly;
"#,
    ));
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![
            1361, 1361, 1361, 1361, 1361, 2339, 1361, 2339, 1361, 2339, 1361, 2339
        ],
        "each explicit or inline type-only value use must report TS1361, while invalid runtime members additionally report TS2339; got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2349),
        "type-only suppression must not replace the augmented callable value with TS2349: {diagnostics:?}"
    );
}

#[test]
fn wrapped_type_only_import_calls_keep_exact_augmentation_runtime() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import type {
    FunctionOnly as ExplicitFunction,
} from "./named-augmentation-home";
import {
    type FunctionOnly as InlineFunction,
} from "./named-augmentation-home";
(ExplicitFunction)();
ExplicitFunction!();
(InlineFunction as typeof InlineFunction)();
"#,
    ));
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![1361, 1361, 1361],
        "parenthesized, non-null, and assertion wrappers must preserve TS1361 without replacing the exact augmented callable with TS2349: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2349),
        "transparent callee wrappers must retain exact augmentation runtime provenance: {diagnostics:?}"
    );
}

#[test]
fn export_type_replay_preserves_suppression_over_augmented_runtime_values() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/named-augmentation-home.ts",
            NAMED_AUGMENTATION_HOME,
        ),
        (
            "/project/named-augmentation-declarations.ts",
            NAMED_AUGMENTATION_DECLARATIONS,
        ),
        (
            "/project/named-augmentation-barrel.ts",
            r#"
export type {
    ConstOnly as ReplayConst,
    FunctionOnly as ReplayFunction,
} from "./named-augmentation-home";
"#,
        ),
        (
            "/project/named-augmentation-consumer.ts",
            r#"
import { ReplayConst, ReplayFunction } from "./named-augmentation-barrel";
ReplayFunction();
ReplayConst.aliasOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![1362, 1362, 2339],
        "an upstream `export type` must replay the home augmentation shape for checking while preserving TS1362 value suppression; got: {diagnostics:?}"
    );
}

#[test]
fn direct_barrel_augmentation_owns_value_despite_type_only_upstream() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/direct-barrel-home.ts",
            "export interface BarrelFunction { upstreamTypeOnly: true }",
        ),
        (
            "/project/direct-barrel.ts",
            r#"export type { BarrelFunction } from "./direct-barrel-home";"#,
        ),
        (
            "/project/direct-barrel-augmentation.ts",
            r#"
import "./direct-barrel";
declare module "./direct-barrel" {
    export function BarrelFunction(): "barrel";
}
"#,
        ),
        (
            "/project/direct-barrel-consumer.ts",
            r#"
import { BarrelFunction } from "./direct-barrel";
const result: "barrel" = BarrelFunction();
BarrelFunction.upstreamTypeOnly;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2339],
        "an augmentation directly targeting the barrel owns a real value there; only the upstream type-only member stays invalid on that value: {diagnostics:?}"
    );
}

#[test]
fn local_type_shadow_does_not_inherit_module_runtime_augmentation() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import "./named-augmentation-home";
type FunctionOnly = { localOnly: true };
FunctionOnly();
type IndependentLocal = { renamedOnly: true }; IndependentLocal(); declare const localValue: FunctionOnly; localValue.instanceOnly;
interface LocalPair { paired: true } declare const LocalPair: { create(): LocalPair }; const paired = LocalPair.create(); paired.paired;
"#,
    ));
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2693, 2693, 2339],
        "local types must keep lexical identity while a real same-scope type/value pair remains usable: {diagnostics:?}"
    );
}

#[test]
fn type_only_named_imports_allow_typeof_queries_of_augmented_values() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import type {
    ConstOnly as ExplicitConst,
    FunctionOnly as ExplicitFunction,
} from "./named-augmentation-home";
import {
    type ConstOnly as InlineConst,
    type FunctionOnly as InlineFunction,
} from "./named-augmentation-home";
type ExplicitFunctionValue = typeof ExplicitFunction;
type ExplicitConstValue = typeof ExplicitConst;
type InlineFunctionValue = typeof InlineFunction;
type InlineConstValue = typeof InlineConst;
declare const explicitFunction: ExplicitFunctionValue;
declare const explicitConst: ExplicitConstValue;
declare const inlineFunction: InlineFunctionValue;
declare const inlineConst: InlineConstValue;
const explicitResult: "function-only" = explicitFunction();
const explicitMeta: "const-meta" = explicitConst.meta;
const inlineResult: "function-only" = inlineFunction();
const inlineMeta: "const-meta" = inlineConst.meta;
"#,
    ));

    assert!(
        diagnostics.is_empty(),
        "`typeof` type queries are type positions: import-type value suppression must not hide the exact augmented value type; got: {diagnostics:?}"
    );
}

#[test]
fn renamed_type_references_preserve_non_generic_and_generic_augmentations() {
    let diagnostics = collect_test_diagnostics(&named_augmentation_files(
        r#"
import type {
    GenericType as RenamedGeneric,
    PlainType as RenamedPlain,
} from "./named-augmentation-home";
declare const plain: RenamedPlain;
const plainBase: "plain" = plain.base;
const plainAugmented: "plain-augmented" = plain.augmented;
declare const generic: RenamedGeneric<"selected">;
const genericValue: "selected" = generic.value;
const genericAugmented: "selected" = generic.augmentedValue;
"#,
    ));

    assert!(
        diagnostics.is_empty(),
        "renaming a non-generic or generic type import must preserve the home augmentation and generic substitution: {diagnostics:?}"
    );
}

#[test]
fn commonjs_js_namespace_paths_append_augmentation_only_runtime_exports() {
    let options = ResolvedCompilerOptions {
        allow_js: true,
        check_js: true,
        module_resolution: Some(crate::config::ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2020,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            module_explicitly_set: true,
            target: tsz_common::common::ScriptTarget::ES2020,
            ..Default::default()
        },
        ..Default::default()
    };
    let diagnostics = collect_test_diagnostics_with_options(
        &[
            ("/project/cjs-runtime-home.js", "exports.native = 1;"),
            (
                "/project/cjs-runtime-augmentation.d.ts",
                r#"
import "./cjs-runtime-home.js";
declare module "./cjs-runtime-home.js" {
    export const AddedRuntime: { readonly meta: "meta" };
}
"#,
            ),
            (
                "/project/cjs-runtime-consumer.ts",
                r#"
import * as star from "./cjs-runtime-home.js";
import requiredHome = require("./cjs-runtime-home.js");
type ImportType = typeof import("./cjs-runtime-home.js");
declare const importType: ImportType;
const starMeta: "meta" = star.AddedRuntime.meta;
const requiredMeta: "meta" = requiredHome.AddedRuntime.meta;
const importTypeMeta: "meta" = importType.AddedRuntime.meta;
"#,
            ),
        ],
        &options,
        std::path::Path::new("/"),
    );

    assert!(
        diagnostics.is_empty(),
        "CommonJS JS surfaces must append exact augmentation-only runtime names for star, require, and `typeof import()` paths; got: {diagnostics:?}"
    );
}

#[test]
fn nodenext_default_namespace_exact_recovers_augmented_type_only_exports() {
    let options = ResolvedCompilerOptions {
        es_module_interop: true,
        allow_synthetic_default_imports: true,
        module_resolution: Some(crate::config::ModuleResolutionKind::NodeNext),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: tsz_common::common::ModuleKind::NodeNext,
            target: tsz_common::common::ScriptTarget::ES2020,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            es_module_interop: true,
            allow_synthetic_default_imports: true,
            module: tsz_common::common::ModuleKind::NodeNext,
            module_explicitly_set: true,
            target: tsz_common::common::ScriptTarget::ES2020,
            ..Default::default()
        },
        ..Default::default()
    };
    let diagnostics = collect_test_diagnostics_with_options(
        &[
            (
                "/project/default-runtime-home.cts",
                r#"
export interface NativeInterface { base: "interface" }
export type NativeAlias = { base: "alias" };
"#,
            ),
            (
                "/project/default-runtime-augmentation.d.ts",
                r#"
import "./default-runtime-home.cjs";
declare module "./default-runtime-home.cjs" {
    namespace NativeInterface { const meta: "interface-meta"; }
    namespace NativeAlias { const meta: "alias-meta"; }
    const AddedRuntime: { readonly meta: "added-meta" };
}
"#,
            ),
            (
                "/project/default-runtime-consumer.mts",
                r#"
import home from "./default-runtime-home.cjs";
const interfaceMeta: "interface-meta" = home.NativeInterface.meta;
const aliasMeta: "alias-meta" = home.NativeAlias.meta;
const addedMeta: "added-meta" = home.AddedRuntime.meta;
"#,
            ),
        ],
        &options,
        std::path::Path::new("/"),
    );

    assert!(
        diagnostics.is_empty(),
        "NodeNext CommonJS default namespaces must exact-recover instantiated value companions for native type-only exports and append augmentation-only runtime names; got: {diagnostics:?}"
    );
}

#[test]
fn augmentation_concrete_values_outrank_namespace_companions_in_both_orders() {
    let diagnostics = collect_test_diagnostics(&[
        ("/project/value-order-home.ts", "export {};"),
        (
            "/project/value-order-augmentation.d.ts",
            r#"
import "./value-order-home";
declare module "./value-order-home" {
    export namespace NamespaceFirstClass {
        export const staticOnly: "namespace-first-static";
    }
    export class NamespaceFirstClass {
        instanceOnly: "namespace-first-instance";
    }

    export class ClassFirstClass {
        instanceOnly: "class-first-instance";
    }
    export namespace ClassFirstClass {
        export const staticOnly: "class-first-static";
    }

    export namespace NamespaceFirstEnum {
        export const staticOnly: "namespace-first-enum-static";
    }
    export enum NamespaceFirstEnum { Member }

    export enum EnumFirstEnum { Member }
    export namespace EnumFirstEnum {
        export const staticOnly: "enum-first-static";
    }
}
"#,
        ),
        (
            "/project/value-order-consumer.ts",
            r#"
import * as home from "./value-order-home";

const namespaceFirstInstance = new home.NamespaceFirstClass();
const namespaceFirstInstanceValue: "namespace-first-instance" =
    namespaceFirstInstance.instanceOnly;
const namespaceFirstStatic: "namespace-first-static" =
    home.NamespaceFirstClass.staticOnly;

const classFirstInstance = new home.ClassFirstClass();
const classFirstInstanceValue: "class-first-instance" =
    classFirstInstance.instanceOnly;
const classFirstStatic: "class-first-static" = home.ClassFirstClass.staticOnly;

const namespaceFirstEnumMember: home.NamespaceFirstEnum =
    home.NamespaceFirstEnum.Member;
const namespaceFirstEnumStatic: "namespace-first-enum-static" =
    home.NamespaceFirstEnum.staticOnly;
const enumFirstMember: home.EnumFirstEnum = home.EnumFirstEnum.Member;
const enumFirstStatic: "enum-first-static" = home.EnumFirstEnum.staticOnly;

home.NamespaceFirstClass.instanceOnly;
namespaceFirstInstance.staticOnly;
home.ClassFirstClass.instanceOnly;
classFirstInstance.staticOnly;
const namespaceFirstEnumObject: home.NamespaceFirstEnum =
    home.NamespaceFirstEnum;
const enumFirstObject: home.EnumFirstEnum = home.EnumFirstEnum;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2339, 2339, 2339, 2339, 2322, 2322],
        "concrete class/enum runtime identity must survive a namespace companion in either declaration order; got: {diagnostics:?}"
    );
}

#[test]
fn split_augmentation_type_and_value_meanings_are_order_independent() {
    let home = ("/project/split/index.ts", "export {};");
    let value_augmentation = (
        "/project/split/value-augmentation.d.ts",
        r#"
import "./index";
declare module "./index" {
    export function DualSurface(value: string): "called";
}
"#,
    );
    let type_augmentation = (
        "/project/split/type-augmentation.d.ts",
        r#"
import "./index";
declare module "./index" {
    export interface DualSurface { typed: true }
}
"#,
    );
    let consumer = (
        "/project/split-consumer.ts",
        r#"
import * as surface from "./split/index";
const called: "called" = surface.DualSurface("");
const typed: surface.DualSurface = { typed: true };
typed.typed;
"#,
    );
    let value_first = [home, value_augmentation, type_augmentation, consumer];
    let type_first = [home, type_augmentation, value_augmentation, consumer];

    for (order, files) in [
        ("value-first", &value_first[..]),
        ("type-first", &type_first[..]),
    ] {
        let diagnostics = collect_test_diagnostics(files);
        assert!(
            diagnostics.is_empty(),
            "split augmentation meanings must retain their independent type/value owners in {order} order; got: {diagnostics:?}"
        );
    }
}

#[test]
fn augmentation_only_interface_type_is_exact_target_and_direct_member_scoped() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/a/index.ts",
            r#"
export interface Native { native: "A" }
export namespace Container {
    export interface Shared { nested: true }
}
"#,
        ),
        ("/project/b/index.ts", "export {};"),
        (
            "/project/b/augmentation.d.ts",
            r#"
import "./index";
declare module "./index" {
    export interface Shared { owner: "B" }
}
"#,
        ),
        (
            "/project/a/augmentation.d.ts",
            r#"
import "./index";
declare module "./index" {
    export interface Shared { owner: "A" }
}
"#,
        ),
        (
            "/project/consumer.ts",
            r#"
import * as surface from "./a/index";
const exact: surface.Shared = { owner: "A" };
const wrong: surface.Shared = { owner: "B" };
const native: surface.Native = { native: "A" };
const nested: surface.Container.Shared = { nested: true };
exact.owner;
native.native;
nested.nested;
"#,
        ),
    ]);

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2322],
        "an augmentation-only interface must stay on its resolved target without stealing foreign/nested/native members; got: {diagnostics:?}"
    );
}

#[test]
fn exact_runtime_augmentation_values_are_scoped_to_their_relative_target() {
    let a_index = ("/project/a/index.ts", "export {};");
    let a_augmentation = (
        "/project/a/augmentation.d.ts",
        r#"
import "./index";
interface ScopedClassBase { inheritedOwner: "A" }
declare module "./index" {
    export const tag: "A";
    export namespace RuntimeBox {
        export const owner: "A";
    }
    export class ScopedClass { classOwner: "A" }
    export interface ScopedClass extends ScopedClassBase {
        interfaceOwner: "A";
    }
}
"#,
    );
    let b_index = ("/project/b/index.ts", "export {};");
    let b_augmentation = (
        "/project/b/augmentation.d.ts",
        r#"
import "./index";
interface ScopedClassBase { inheritedOwner: "B" }
declare module "./index" {
    export const tag: "B";
    export namespace RuntimeBox {
        export const owner: "B";
    }
    export class ScopedClass { classOwner: "B" }
    export interface ScopedClass extends ScopedClassBase {
        interfaceOwner: "B";
    }
}
"#,
    );
    let consumer = (
        "/project/consumer.ts",
        r#"
import * as a from "./a/index";
import * as b from "./b/index";
const aTag: "A" = a.tag;
const bTag: "B" = b.tag;
const aOwner: "A" = a.RuntimeBox.owner;
const bOwner: "B" = b.RuntimeBox.owner;
const aClass = new a.ScopedClass();
const bClass = new b.ScopedClass();
const aClassOwner: "A" = aClass.classOwner;
const bClassOwner: "B" = bClass.classOwner;
const aInterfaceOwner: "A" = aClass.interfaceOwner;
const bInterfaceOwner: "B" = bClass.interfaceOwner;
const aInheritedOwner: "A" = aClass.inheritedOwner;
const bInheritedOwner: "B" = bClass.inheritedOwner;
"#,
    );
    let a_first = [a_index, a_augmentation, b_index, b_augmentation, consumer];
    let b_first = [b_index, b_augmentation, a_index, a_augmentation, consumer];

    for (order, files) in [("a-first", &a_first[..]), ("b-first", &b_first[..])] {
        let diagnostics = collect_test_diagnostics(files);
        assert!(
            diagnostics.is_empty(),
            "exact runtime declarations with the same relative key must resolve against their declaring target in {order} program order; got: {diagnostics:?}"
        );
    }
}

#[test]
fn inherited_module_augmentation_construct_uses_declaring_scope() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/inherited-construct-home.ts",
            r#"
export class InheritedConstructableClass {
    constructor(value: number) { void value; }
}
export interface GenericInheritedConstructable<T> {}
export interface NestedConstructBase {}
export interface NestedInheritedConstructable {}
"#,
        ),
        (
            "/project/inherited-construct-augmentation.ts",
            r#"
import type {
    GenericInheritedConstructable,
    InheritedConstructableClass,
    NestedInheritedConstructable,
} from "./inherited-construct-home";
interface AugmentationConstructBase {
    new (value: string): { selected: "inherited" };
}
interface GenericAugmentationConstructBase<V> {
    new (value: V): { value: V };
}
declare module "./inherited-construct-home" {
    interface InheritedConstructableClass extends AugmentationConstructBase {}
    interface GenericInheritedConstructable<T>
        extends GenericAugmentationConstructBase<T> {}
    interface NestedConstructBase {
        nestedMarker: "nested";
        new (value: boolean): { selected: "nested" };
    }
    interface NestedInheritedConstructable extends NestedConstructBase {
        new (value: "direct"): { selected: "nested-direct" };
        new (value: "direct"): { selected: "nested-direct" };
    }
}
declare const localInstance: InheritedConstructableClass;
const localResult: { selected: "inherited" } = new localInstance("local");
declare const localGeneric: GenericInheritedConstructable<"local-generic">;
const localGenericResult: { value: "local-generic" } =
    new localGeneric("local-generic");
declare const localNested: NestedInheritedConstructable;
const localNestedResult: { selected: "nested" } = new localNested(true);
const localNestedDirectResult: { selected: "nested-direct" } =
    new localNested("direct");
const localNestedMarker: "nested" = localNested.nestedMarker;
"#,
        ),
        (
            "/project/inherited-construct-consumer.ts",
            r#"
import { InheritedConstructableClass } from "./inherited-construct-home";
import type {
    GenericInheritedConstructable,
    NestedInheritedConstructable,
} from "./inherited-construct-home";
new InheritedConstructableClass("wrong-side");
declare const foreignInstance: InheritedConstructableClass;
const foreignResult: { selected: "inherited" } = new foreignInstance("foreign");
declare const foreignGeneric: GenericInheritedConstructable<"foreign-generic">;
const foreignGenericResult: { value: "foreign-generic" } =
    new foreignGeneric("foreign-generic");
declare const foreignNested: NestedInheritedConstructable;
const foreignNestedResult: { selected: "nested" } = new foreignNested(false);
const foreignNestedDirectResult: { selected: "nested-direct" } =
    new foreignNested("direct");
const foreignNestedMarker: "nested" = foreignNested.nestedMarker;
"#,
        ),
    ]);
    let codes: Vec<_> = diagnostics.iter().map(|diag| diag.code).collect();

    assert_eq!(
        codes,
        vec![2345],
        "inherited augmentation construction belongs to the instance side, resolves local/generic/nested Base bindings in both local and foreign consumers, and must not alter the class value constructor; got: {diagnostics:?}"
    );
}

#[test]
fn module_augmentation_inherits_generic_array_surface() {
    let lib_files = tsz::checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "expected the ES5 Array declarations");
    let diagnostics = collect_test_diagnostics_with_lib_files(
        &[
            (
                "/project/array-augmentation-home.ts",
                r#"
export interface ArrayAugmentation<T> {
    own: T;
}
export interface TupleAugmentation<T, U> {
    ownTuple: T;
}
"#,
            ),
            (
                "/project/array-augmentation.ts",
                r#"
import type {
    ArrayAugmentation,
    TupleAugmentation,
} from "./array-augmentation-home";
import "./array-augmentation-home";
type TupleBase<T, U> = [T, U?];
declare module "./array-augmentation-home" {
    interface ArrayAugmentation<T> extends Array<T> {}
    interface TupleAugmentation<T, U> extends TupleBase<T, U> {}
}
declare const localValue: ArrayAugmentation<string>;
const localLength: number = localValue.length;
const localMapped: string[] = localValue.map(value => value);
const localFirst: string = localValue[0];
declare const localTuple: TupleAugmentation<string, number>;
const localTupleFirst: string = localTuple[0];
const localTupleSecond: number | undefined = localTuple[1];
const localTupleLength: 1 | 2 = localTuple.length;
"#,
            ),
            (
                "/project/array-augmentation-consumer.ts",
                r#"
import type {
    ArrayAugmentation,
    TupleAugmentation,
} from "./array-augmentation-home";
declare const foreignValue: ArrayAugmentation<number>;
const foreignLength: number = foreignValue.length;
const foreignMapped: number[] = foreignValue.map(value => value);
const foreignFirst: number = foreignValue[0];
const foreignOwn: number = foreignValue.own;
declare const foreignTuple: TupleAugmentation<number, string>;
const foreignTupleFirst: number = foreignTuple[0];
const foreignTupleSecond: string | undefined = foreignTuple[1];
const foreignTupleLength: 1 | 2 = foreignTuple.length;
const foreignTupleOwn: number = foreignTuple.ownTuple;
"#,
            ),
        ],
        &lib_files,
    );

    assert!(
        diagnostics.is_empty(),
        "array/tuple heritage must contribute named, fixed-slot, literal-length, and numeric-index surfaces in both the declaring and consuming files; got: {diagnostics:?}"
    );
}

#[test]
fn module_augmentation_inherits_callable_and_string_index_surfaces() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/call-index-augmentation-home.ts",
            r#"
export interface CallableAugmentation {
    ownCallProperty: "call-home";
}
export interface StringIndexAugmentation {
    ownIndexProperty: number;
}
export interface CallableIndexedAugmentation {}
"#,
        ),
        (
            "/project/call-index-augmentation.ts",
            r#"
import type {
    CallableAugmentation,
    CallableIndexedAugmentation,
    StringIndexAugmentation,
} from "./call-index-augmentation-home";
import "./call-index-augmentation-home";

interface CallableBase {
    (value: string): { selected: "called" };
    inheritedCallProperty: "call-base";
}
interface StringIndexBase {
    [key: string]: number;
}
interface CallableIndexBase {
    (value: string): { selected: "call-indexed" };
    [key: string]: number;
    [key: symbol]: boolean;
}

declare module "./call-index-augmentation-home" {
    interface CallableAugmentation extends CallableBase {}
    interface CallableIndexedAugmentation extends CallableIndexBase {}
    interface StringIndexAugmentation extends StringIndexBase {}
}

declare const localCallable: CallableAugmentation;
const localCalled: { selected: "called" } = localCallable("local");
const localCallProperty: "call-base" = localCallable.inheritedCallProperty;
declare const localIndexed: StringIndexAugmentation;
const localStringIndex: number = localIndexed["local-key"];
declare const localSymbolKey: symbol;
declare const localCallableIndexed: CallableIndexedAugmentation;
const localCallableIndexedResult: { selected: "call-indexed" } =
    localCallableIndexed("local");
const localCallableStringIndex: number = localCallableIndexed["local-key"];
const localCallableSymbolIndex: boolean = localCallableIndexed[localSymbolKey];
"#,
        ),
        (
            "/project/call-index-augmentation-consumer.ts",
            r#"
import type {
    CallableAugmentation,
    CallableIndexedAugmentation,
    StringIndexAugmentation,
} from "./call-index-augmentation-home";

declare const foreignCallable: CallableAugmentation;
const foreignCalled: { selected: "called" } = foreignCallable("foreign");
const foreignCallProperty: "call-base" = foreignCallable.inheritedCallProperty;
const foreignOwnCallProperty: "call-home" = foreignCallable.ownCallProperty;
declare const foreignIndexed: StringIndexAugmentation;
const foreignStringIndex: number = foreignIndexed["foreign-key"];
const foreignOwnIndexProperty: number = foreignIndexed.ownIndexProperty;
declare const foreignSymbolKey: symbol;
declare const foreignCallableIndexed: CallableIndexedAugmentation;
const foreignCallableIndexedResult: { selected: "call-indexed" } =
    foreignCallableIndexed("foreign");
const foreignCallableStringIndex: number = foreignCallableIndexed["foreign-key"];
const foreignCallableSymbolIndex: boolean =
    foreignCallableIndexed[foreignSymbolKey];
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "callable and independent string/symbol-index heritage must survive module augmentation in both the declaring and consuming files; got: {diagnostics:?}"
    );
}

#[test]
fn cross_file_ts2403_child_retains_construct_merge_context() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/nested-child-class.ts",
            r#"
interface NestedChildResult { selected: "nested" }
class NestedChildCtor {}
var sharedCtor: NestedChildCtor;
"#,
        ),
        (
            "/project/nested-child-interface.ts",
            r#"
interface NestedChildCtor {
    new (value: string): NestedChildResult;
}
"#,
        ),
        (
            "/project/nested-child-comparison.ts",
            r#"
var sharedCtor: {
    new (value: string): NestedChildResult;
};
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "the nested checker used for cross-file TS2403 comparison must retain every construct declaration group; got: {diagnostics:?}"
    );
}

#[test]
fn foreign_construct_group_uses_owning_module_scope() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/scoped-construct-home.ts",
            r#"
class ScopedConstruct {}
"#,
        ),
        (
            "/project/scoped-construct-types.ts",
            r#"
export interface ImportedInput { input: "imported" }
export interface ImportedResult { result: "imported" }
"#,
        ),
        (
            "/project/scoped-construct-augmentation.ts",
            r#"
import type {
    ImportedInput,
    ImportedResult,
} from "./scoped-construct-types";
interface ModuleLocalResult { result: "local" }
interface ModuleLocalConstructBase {
    new (value: "inherited"): ModuleLocalResult;
    inheritedFromLocalBase: true;
}
declare global {
    interface ScopedConstruct extends ModuleLocalConstructBase {
        new (value: ImportedInput): ImportedResult;
        new (value: "local"): ModuleLocalResult;
    }
}
"#,
        ),
        (
            "/project/scoped-construct-consumer.ts",
            r#"
import type {
    ImportedInput,
    ImportedResult,
} from "./scoped-construct-types";
declare const Ctor: ScopedConstruct;
declare const input: ImportedInput;
const importedResult: ImportedResult = new Ctor(input);
const localResult: { result: "local" } = new Ctor("local");
const inheritedResult: { result: "local" } = new Ctor("inherited");
declare const scopedValue: ScopedConstruct;
const inheritedMember: true = scopedValue.inheritedFromLocalBase;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "foreign construct declarations must lower imported and module-local names in their owning binder scope; got: {diagnostics:?}"
    );
}

#[test]
fn cross_file_heritage_type_argument_respects_local_builtin_named_alias() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/local-builtin-name.ts",
            r#"
interface LocalBase<T> { value: T }
type BuiltinIteratorReturn = string;
export interface LocalDerived extends LocalBase<BuiltinIteratorReturn> {}
"#,
        ),
        (
            "/project/local-builtin-name-consumer.ts",
            r#"
import type { LocalDerived } from "./local-builtin-name";
declare const value: LocalDerived;
const selected: string = value.value;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "cross-file heritage type arguments must resolve a module-local alias before compiler builtin policy; got: {diagnostics:?}"
    );
}

#[test]
fn jsx_attribute_comma_expression_survives_into_bind_results() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

    let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
    let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&18007),
        "expected TS18007 in bind-result parse diagnostics, got: {codes:?}"
    );
}

#[test]
fn jsx_attribute_comma_expression_reports_ts18007_in_cli_diagnostics() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        [s: string]: any;
    }
}

const class1 = "foo";
const class2 = "bar";
const elem = <div className={class1, class2}/>;
"#;

    let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&18007),
        "expected CLI diagnostics to include TS18007, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2695),
        "expected CLI diagnostics to include TS2695, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_bind_results() {
    let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
    let result = parallel::parse_and_bind_single("file.tsx".to_string(), source.to_string());
    let less_than_pos = source.find('<').expect("opening angle") as u32;
    let colon_pos = source[less_than_pos as usize + 1..]
        .find(':')
        .map(|offset| less_than_pos + 1 + offset as u32)
        .expect("colon");
    let expr_expected_positions: Vec<u32> = result
        .parse_diagnostics
        .iter()
        .filter(|diag| diag.code == 1109)
        .map(|diag| diag.start)
        .collect();

    assert!(
        expr_expected_positions.contains(&less_than_pos),
        "expected TS1109 at '<', got: {expr_expected_positions:?}"
    );
    assert!(
        expr_expected_positions.contains(&colon_pos),
        "expected TS1109 at ':', got: {expr_expected_positions:?}"
    );
}

#[test]
fn jsx_invalid_namespace_start_keeps_colon_ts1109_in_cli_diagnostics() {
    let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
    let diagnostics = collect_test_diagnostics(&[("file.tsx", source)]);
    let less_than_pos = source.find('<').expect("opening angle") as u32;
    let colon_pos = source[less_than_pos as usize + 1..]
        .find(':')
        .map(|offset| less_than_pos + 1 + offset as u32)
        .expect("colon");
    let expr_expected_positions: Vec<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == 1109)
        .map(|diag| diag.start)
        .collect();

    assert!(
        expr_expected_positions.contains(&less_than_pos),
        "expected CLI TS1109 at '<', got: {diagnostics:?}"
    );
    assert!(
        expr_expected_positions.contains(&colon_pos),
        "expected CLI TS1109 at ':', got: {diagnostics:?}"
    );
}

#[test]
fn test_collect_diagnostics_preserves_mapped_type_nullish_indexed_reads() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let file_path = dir.path().join("main.ts");
    std::fs::write(&file_path, mapped_type_indexed_access_constraint_repro())
        .expect("write source");

    let resolved = resolved_options_for_es2015_strict_test();
    let file_paths = vec![file_path];
    let SourceReadResult {
        sources,
        dependencies: _,
        module_resolutions: _,
        type_reference_errors,
        resolution_mode_errors,
        ..
    } = super::read_source_files(&file_paths, dir.path(), &resolved, None, None)
        .expect("read source files");

    assert!(type_reference_errors.is_empty());
    assert!(resolution_mode_errors.is_empty());

    let disable_default_libs =
        resolved.lib_is_default && super::sources_have_no_default_lib(&sources);
    let lib_paths =
        super::resolve_effective_lib_paths(&resolved, &sources, dir.path(), disable_default_libs)
            .expect("resolve effective lib paths");
    let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
    let lib_files =
        parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs");
    let checker_libs = load_checker_libs(&lib_files);
    let compile_inputs: Vec<_> = sources
        .into_iter()
        .map(|source| {
            (
                source.path.to_string_lossy().into_owned(),
                source.text.unwrap_or_default(),
            )
        })
        .collect();
    let program = parallel::merge_bind_results(parallel::parse_and_bind_parallel_with_libs(
        compile_inputs,
        &lib_files,
    ));

    let type_cache_output = std::sync::Mutex::new(FxHashMap::default());
    let diagnostics = collect_diagnostics(
        &CollectDiagnosticsInput {
            program: &program,
            options: &resolved,
            base_dir: dir.path(),
            reference_path_current_directory: None,
            checker_libs: &checker_libs,
            typescript_dom_replacement_globals: (false, false, false),
            has_deprecation_diagnostics: false,
            collect_compile_stats: false,
        },
        None,
        &type_cache_output,
    )
    .diagnostics;
    let ts18048_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::IS_POSSIBLY_UNDEFINED)
        .count();
    let ts2532_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED)
        .count();
    let ts2722_count = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED
        })
        .count();
    let ts2349_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE)
        .count();

    assert_eq!(
        ts18048_count, 3,
        "Expected collect_diagnostics to preserve three TS18048 diagnostics, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2532_count, 1,
        "Expected one TS2532 for mapped[key].toString(), got: {diagnostics:?}"
    );
    assert_eq!(
        ts2722_count, 1,
        "Expected one TS2722 for mapper[key](o), got: {diagnostics:?}"
    );
    assert_eq!(
        ts2349_count, 0,
        "Did not expect TS2349 for mapper[key](o), got: {diagnostics:?}"
    );
}
