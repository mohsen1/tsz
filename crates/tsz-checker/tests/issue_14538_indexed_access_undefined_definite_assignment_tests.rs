//! Regression tests for #14538: TS2454 ("used before being assigned") must not
//! fire when the declared type includes `undefined` only through an *unevaluated*
//! indexed-access (or alias) union member.
//!
//! ## Structural rule
//!
//! `skip_definite_assignment_for_type` suppresses TS2454 when the declared type
//! contains `undefined`. It checked the RAW declared type, so for a union member
//! that is an unevaluated `IndexAccess(W, 'opt')` (an optional property), the
//! `undefined` the indexed access *resolves to* was invisible and the suppression
//! did not apply — a spurious TS2454. tsc gates TS2454 on the resolved/apparent
//! type, so the check now evaluates the declared type before concluding it cannot
//! be `undefined`. Evaluation must NOT manufacture `undefined`: a *required*
//! property's indexed access (`W['req']`) still resolves without `undefined`, so
//! TS2454 must still fire there.
//!
//! Witness: zustand `src/middleware/devtools.ts:209`
//! (`let extensionConnector: (typeof window)['__REDUX_DEVTOOLS_EXTENSION__'] | false`).
//!
//! The rule is structural (indexed-access resolution), not keyed on identifier
//! spellings — see the renamed-binder test.

use tsz_checker::test_utils::check_source_strict_messages_without_missing_libs as diags;

const TS2454: u32 = 2454;

fn count_2454(source: &str) -> usize {
    diags(source).iter().filter(|(c, _)| *c == TS2454).count()
}

const PRELUDE: &str = "\
interface W { opt?: { connect(): void }; req: { go(): void }; }\n\
declare const w: W;\n\
declare const cond: boolean;\n";

/// POSITIVE: `undefined` reachable only via the optional-property indexed access
/// `W['opt']` — assigned in a `try`, read after a guard. Must be clean (tsc: ok).
#[test]
fn indexed_access_optional_property_union_suppresses_ts2454() {
    let src = format!(
        "{PRELUDE}\
function f() {{\n\
  let x: W['opt'] | false;\n\
  try {{ x = cond && w.opt; }} catch {{}}\n\
  if (!x) return undefined;\n\
  return x;\n\
}}\n"
    );
    assert_eq!(
        count_2454(&src),
        0,
        "W['opt'] resolves to `{{connect()}} | undefined`, so the read is benign: {:?}",
        diags(&src)
    );
}

/// Same shape through a type ALIAS over the indexed access — evaluation must
/// expand the alias too.
#[test]
fn alias_over_indexed_access_optional_property_suppresses_ts2454() {
    let src = format!(
        "{PRELUDE}\
type Opt = W['opt'];\n\
function f() {{\n\
  let x: Opt | false;\n\
  try {{ x = cond && w.opt; }} catch {{}}\n\
  if (!x) return undefined;\n\
  return x;\n\
}}\n"
    );
    assert_eq!(
        count_2454(&src),
        0,
        "alias of W['opt'] must also suppress TS2454: {:?}",
        diags(&src)
    );
}

/// CONTROL A: a plain `string` (no `undefined`) read before assignment must
/// STILL report TS2454 — the fix must not blanket-suppress.
#[test]
fn plain_type_without_undefined_still_reports_ts2454() {
    let src = format!(
        "{PRELUDE}\
function f(): string {{\n\
  let y: string;\n\
  if (cond) {{ return y; }}\n\
  y = \"v\";\n\
  return y;\n\
}}\n"
    );
    assert_eq!(
        count_2454(&src),
        1,
        "string has no undefined: TS2454 must fire: {:?}",
        diags(&src)
    );
}

/// CONTROL B: a REQUIRED property indexed access (`W['req']`) resolves WITHOUT
/// `undefined`, so TS2454 must still fire. This guards against the evaluation
/// step manufacturing `undefined`.
#[test]
fn indexed_access_required_property_still_reports_ts2454() {
    let src = format!(
        "{PRELUDE}\
function f() {{\n\
  let z: W['req'];\n\
  if (cond) {{ return z; }}\n\
  z = w.req;\n\
  return z;\n\
}}\n"
    );
    assert_eq!(
        count_2454(&src),
        1,
        "W['req'] resolves to `{{go()}}` (no undefined): TS2454 must fire: {:?}",
        diags(&src)
    );
}

/// ADJACENT: a direct `... | undefined` annotation was already handled (the raw
/// type contains `undefined`); pin it so the fast path stays correct.
#[test]
fn direct_undefined_union_suppresses_ts2454() {
    let src = format!(
        "{PRELUDE}\
function f() {{\n\
  let a: {{ connect(): void }} | undefined;\n\
  try {{ a = w.opt; }} catch {{}}\n\
  if (!a) return undefined;\n\
  return a;\n\
}}\n"
    );
    assert_eq!(
        count_2454(&src),
        0,
        "direct `| undefined` must suppress TS2454: {:?}",
        diags(&src)
    );
}

/// ANTI-HARDCODING: the rule is structural, not keyed on the spellings `W` /
/// `opt` / `req`. Re-run the positive + the required-property control with
/// renamed binders.
#[test]
fn rule_is_binder_name_agnostic() {
    for (iface, opt, req, val) in [
        ("Cfg", "maybe", "always", "v"),
        ("State", "handler", "store", "u"),
    ] {
        let prelude = format!(
            "interface {iface} {{ {opt}?: {{ connect(): void }}; {req}: {{ go(): void }}; }}\n\
             declare const obj: {iface};\n\
             declare const cond: boolean;\n"
        );

        let positive = format!(
            "{prelude}\
function f() {{\n\
  let x: {iface}['{opt}'] | false;\n\
  try {{ x = cond && obj.{opt}; }} catch {{}}\n\
  if (!x) return undefined;\n\
  return x;\n\
}}\n"
        );
        assert_eq!(
            count_2454(&positive),
            0,
            "[{iface}/{opt}] optional indexed access must suppress TS2454: {:?}",
            diags(&positive)
        );

        let required = format!(
            "{prelude}\
function f() {{\n\
  let z: {iface}['{req}'] = obj.{req};\n\
  let w2: {iface}['{req}'];\n\
  if (cond) {{ return w2; }}\n\
  w2 = z;\n\
  return w2;\n\
}}\n"
        );
        let _ = val;
        assert_eq!(
            count_2454(&required),
            1,
            "[{iface}/{req}] required indexed access must still report TS2454: {:?}",
            diags(&required)
        );
    }
}
