use super::strict_diagnostics_for;

#[test]
fn inferred_return_conditional_preserves_literal_union() {
    let diagnostics = strict_diagnostics_for(
        r#"
type Same<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;
type Must<T extends true> = T;
type InferRet<T> = T extends (...args: any[]) => infer R ? R : never;

const chooseNumber = (flag: boolean) => flag ? 1 : 2;
function chooseString(flag: boolean) {
  return flag ? "left" : "right";
}
const chooseNullish = (flag: boolean) => flag ? true : undefined;
function singleLiteral() {
  return 1;
}

type cases = [
  Must<Same<InferRet<typeof chooseNumber>, 1 | 2>>,
  Must<Same<InferRet<typeof chooseString>, "left" | "right">>,
  Must<Same<InferRet<typeof chooseNullish>, true | undefined>>,
  Must<Same<InferRet<typeof singleLiteral>, number>>,
];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "conditional return inference should preserve branch literal unions; got: {diagnostics:?}"
    );
}
