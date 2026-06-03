//! `new Derived(args)` inside `Derived`'s own static-property initializer
//! must see the inherited construct signatures. Without this, the rough
//! partial constructor type used during static-member processing falls back
//! to a default 0-arg constructor, producing a false TS2554.

use crate::test_utils::check_source_diagnostics;

#[test]
fn three_class_hierarchy_with_instance_field_aliases_and_static_create() {
    let diags = check_source_diagnostics(
        r#"
export interface ZodTypeDef { errorMap?: any; }

export enum ZodFirstPartyTypeKind {
  ZodString = "ZodString",
  ZodNumber = "ZodNumber",
  ZodEffects = "ZodEffects",
}

export type ErrMessage = { message?: string } | string;

export abstract class ZodType<Output, Def extends ZodTypeDef = ZodTypeDef, Input = Output> {
  readonly _def!: Def;
  constructor(def: Def) {
    (this as any)._def = def;
  }
  abstract _parse(data: any): boolean;

  _refinement(refinement: any): ZodEffects<this> {
    return new ZodEffects({
      schema: this,
      typeName: ZodFirstPartyTypeKind.ZodEffects,
      effect: { type: "refinement", refinement },
    }) as any;
  }
}

export interface ZodEffectsDef<T extends ZodTypeAny> extends ZodTypeDef {
  schema: T;
  typeName: ZodFirstPartyTypeKind.ZodEffects;
  effect: any;
}
export type ZodTypeAny = ZodType<any, any, any>;

export class ZodEffects<T extends ZodTypeAny, Output = any, Input = any>
  extends ZodType<Output, ZodEffectsDef<T>, Input> {
  _parse(data: any): boolean { return true; }
}

export interface ZodNumberDef extends ZodTypeDef {
  checks: number[];
  typeName: ZodFirstPartyTypeKind.ZodNumber;
}

export class ZodNumber extends ZodType<number, ZodNumberDef> {
  _parse(data: any): boolean { return typeof data === "number"; }

  gte(value: number, message?: ErrMessage) { return this; }
  min = this.gte;

  lte(value: number, message?: ErrMessage) { return this; }
  max = this.lte;

  static create = (): ZodNumber => {
    return new ZodNumber({ checks: [], typeName: ZodFirstPartyTypeKind.ZodNumber });
  };
}
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        ts2554.is_empty(),
        "Expected no TS2554 in Zod-style 3-class hierarchy; got: {ts2554:?}"
    );
}

#[test]
fn new_derived_in_static_property_initializer_inherits_base_construct_arity() {
    let diags = check_source_diagnostics(
        r#"
class Base<Def> {
    constructor(def: Def) {}
}

class Derived extends Base<{ count: number }> {
    static create = (): Derived => {
        return new Derived({ count: 1 });
    };
}
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        ts2554.is_empty(),
        "Expected no TS2554 for `new Derived(...)` in static-property initializer; got: {diags:?}"
    );
}

#[test]
fn new_derived_in_static_method_inherits_base_construct_arity() {
    let diags = check_source_diagnostics(
        r#"
class Base<Def> {
    constructor(def: Def) {}
}

class Derived extends Base<{ count: number }> {
    static create(): Derived {
        return new Derived({ count: 1 });
    }
}
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        ts2554.is_empty(),
        "Expected no TS2554 for `new Derived(...)` in static method; got: {diags:?}"
    );
}
