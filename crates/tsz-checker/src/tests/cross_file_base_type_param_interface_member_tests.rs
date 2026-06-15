//! Cross-arena base type parameter bound to a cross-file interface (#13044, #13484).
//!
//! When a generic base class declares a member typed by one of its own type
//! parameters (`abstract class Base<Out, Def extends BaseDef = BaseDef> {
//! readonly _def!: Def }`), and a derived class binds that parameter to an
//! interface (`class Str extends Base<string, StrDef>`), reading the inherited
//! member on a derived instance (`this._def.checks`) must resolve `Def` to the
//! derived class's type argument (`StrDef`).
//!
//! Historically, when the derived class and the bound interface were observed
//! across arenas (multi-file, barrel-heavy graphs), the binder recorded a
//! declaration `NodeIndex` for the interface that did not resolve in the
//! importing arena and provided no `declaration_arenas` bridge, so the
//! interface body degraded to `TypeId::ERROR`. That `error` then surfaced in a
//! type-argument slot through inheritance — zod's
//! `ZodType<_, Def>._def` with `Def = ZodStringDef` rendered `{ checks:
//! error[] }` (the #13484 cross-canary witness). The interface body is a pure
//! function of its own declaration regardless of the referencing file, so it is
//! now recovered by lowering the declaration in the arena that actually holds
//! it (and merging its `extends`-bases from that same arena).
//!
//! Binder names are varied (`Schema`/`StrShape`, `Parser`/`Cfg`) to prove the
//! recovery follows the type shape, not any identifier or file-name string.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

/// Any diagnostic whose rendered message mentions the internal `error` type in
/// a member/argument position is the degradation witness.
fn error_type_leak_messages(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.message_text.contains("error[]") || d.message_text.contains(": error"))
        .map(|d| d.message_text.to_string())
        .collect()
}

fn assignability_errors(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                || d.code == diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE
        })
        .map(|d| (d.code, d.message_text.to_string()))
        .collect()
}

/// The core #13484 witness: a derived class in a separate file reads a base
/// member typed by a base type parameter that the derived class binds to a
/// cross-file interface. The interface (and its `extends`-base members) must
/// resolve so the spread `{ ...this._def, checks: [...this._def.checks, c] }`
/// is `{ checks: StrShape["checks"] }`, never `{ checks: error[] }`.
#[test]
fn derived_reads_base_type_param_bound_to_cross_file_interface() {
    let schema = r#"
export interface BaseShape {
  label?: string;
  note?: string;
}
export interface StrShape extends BaseShape {
  checks: number[];
  kind: "str";
}
export abstract class Schema<Out, Shape extends BaseShape = BaseShape> {
  readonly _shape!: Shape;
  constructor(shape: Shape) {
    this._shape = shape;
  }
}
"#;
    let main = r#"
import { Schema, StrShape } from "./schema";
export class Str extends Schema<string, StrShape> {
  add(c: number): Str {
    return new Str({ ...this._shape, checks: [...this._shape.checks, c] });
  }
}
"#;
    let diags = check(&[("./schema.ts", schema), ("./main.ts", main)], "./main.ts");

    let leaks = error_type_leak_messages(&diags);
    assert!(
        leaks.is_empty(),
        "base type parameter bound to a cross-file interface degraded to `error`: {leaks:?}",
    );
    let assignability = assignability_errors(&diags);
    assert!(
        assignability.is_empty(),
        "expected the recovered interface to relate cleanly, got: {assignability:?}",
    );
}

/// Same shape, different binder names and member spellings, proving the
/// recovery is structural rather than keyed on any identifier.
#[test]
fn renamed_derived_reads_base_type_param_bound_to_cross_file_interface() {
    let lib = r#"
export interface CfgBase {
  message?: string;
}
export interface NumCfg extends CfgBase {
  bounds: number[];
  tag: "num";
}
export abstract class Parser<Value, Cfg extends CfgBase = CfgBase> {
  readonly _cfg!: Cfg;
  constructor(cfg: Cfg) {
    this._cfg = cfg;
  }
}
"#;
    let app = r#"
import { Parser, NumCfg } from "./lib";
export class NumParser extends Parser<number, NumCfg> {
  widen(b: number): NumParser {
    return new NumParser({ ...this._cfg, bounds: [...this._cfg.bounds, b] });
  }
}
"#;
    let diags = check(&[("./lib.ts", lib), ("./app.ts", app)], "./app.ts");

    let leaks = error_type_leak_messages(&diags);
    assert!(
        leaks.is_empty(),
        "renamed cross-file base-member interface degraded to `error`: {leaks:?}",
    );
    let assignability = assignability_errors(&diags);
    assert!(
        assignability.is_empty(),
        "expected the recovered interface to relate cleanly, got: {assignability:?}",
    );
}

/// The recovered interface must carry its `extends`-base members so a weak
/// (all-optional) base type still relates by structural inheritance rather than
/// firing TS2559 "no properties in common". This guards the heritage-merge half
/// of the recovery specifically.
#[test]
fn cross_file_interface_recovery_preserves_extends_base_members() {
    let schema = r#"
export interface WeakBase {
  errorMap?: string;
  description?: string;
}
export interface FullShape extends WeakBase {
  checks: number[];
}
export abstract class Holder<Shape extends WeakBase = WeakBase> {
  readonly _shape!: Shape;
}
"#;
    let main = r#"
import { Holder, FullShape, WeakBase } from "./schema";
export class FullHolder extends Holder<FullShape> {
  read(): WeakBase {
    return this._shape;
  }
}
"#;
    let diags = check(&[("./schema.ts", schema), ("./main.ts", main)], "./main.ts");

    let assignability = assignability_errors(&diags);
    assert!(
        assignability.is_empty(),
        "recovered interface lost its extends-base members (weak-type mismatch): {assignability:?}",
    );
}
