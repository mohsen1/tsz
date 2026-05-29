#[test]
fn ts7022_ts7023_do_not_fire_for_void_expression_return_operand() {
    let diags = check_source_diagnostics(
        r#"
type HowlErrorCallback = (soundId: number, error: unknown) => void;

interface HowlOptions {
  onplayerror?: HowlErrorCallback | undefined;
}

class Howl {
  constructor(public readonly options: HowlOptions) {}
  once(name: "unlock", fn: () => void) {
    console.log(name, fn);
  }
}

const instance = new Howl({
  onplayerror: () => void instance.once("unlock", () => {}),
});
"#,
    );
    let circularity: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7022 | 7023))
        .collect();
    assert!(
        circularity.is_empty(),
        "Expected no TS7022/TS7023 for self-reference under void return expression, got: {:?}",
        diagnostic_ref_summaries(&circularity)
    );
}

#[test]
fn ts7023_no_false_positive_when_property_key_matches_outer_var() {
    // The key in an object literal (also a non-value name position) must not
    // be treated as a lexical reference to a same-named outer variable.
    let diags = check_source_diagnostics(
        r#"
const wrap = (x: number) => ({ wrap: x });
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 when an object property key matches the enclosing variable name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts2322_no_false_positive_merged_type_alias_and_const_return() {
    // Two name variants guard against name-hardcoding regressions (§25).
    for source in [
        r#"
type Foo = { type: "foo" };
const Foo = {
  make: (): Foo => {
    return { type: "foo" };
  }
};
"#,
        r#"
type MyAlias = { kind: "ok" };
const MyAlias = {
  build: (): MyAlias => {
    return { kind: "ok" };
  }
};
"#,
    ] {
        let diags = check_source_diagnostics(source);
        let ts2322 = diagnostics_with_code(&diags, 2322);
        assert!(
            ts2322.is_empty(),
            "Expected no TS2322 for merged type-alias+const return, got: {:?}",
            diagnostic_messages(&ts2322)
        );
    }
}

#[test]
fn ts2322_real_error_still_reported_for_merged_type_alias_and_const_wrong_return() {
    let diags = check_source_diagnostics(
        r#"
type Status = { code: "ok" };
const Status = {
  make: (): Status => {
    return { code: "wrong" };
  }
};
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for wrong literal in merged type-alias+const return, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}
