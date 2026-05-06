//! Regression tests for issue #3139: deeply nested destructured function
//! parameters were misclassified as local variables by `noUnusedLocals` because
//! the checker only walked a fixed ancestor depth before deciding ownership.
//!
//! tsc only reports unused destructured parameters under `noUnusedParameters`,
//! never under `noUnusedLocals`. The walk in `is_parameter_declaration` must
//! reach the enclosing `Parameter` regardless of binding-pattern nesting depth.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn check_with_no_unused_locals(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_locals: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn shallow_destructured_parameter_is_exempt_from_no_unused_locals() {
    let codes = check_with_no_unused_locals(
        r#"
export function f({ l }: { l: string }) {}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for shallow destructured parameter. Got: {codes:?}"
    );
}

#[test]
fn moderately_nested_destructured_parameter_is_exempt_from_no_unused_locals() {
    let codes = check_with_no_unused_locals(
        r#"
export function f({
  a: {
    b: {
      c: { d }
    }
  }
}: {
  a: {
    b: {
      c: { d: string }
    }
  }
}) {}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for moderately nested destructured parameter. Got: {codes:?}"
    );
}

#[test]
fn deeply_nested_destructured_parameter_is_exempt_from_no_unused_locals() {
    // 12 levels of nesting — the original bug repro from issue #3139.
    // The previous fixed-depth walk (cap of 20 ancestors counting both
    // BindingElement and BindingPattern hops) gave up before hitting the
    // Parameter and falsely reported the innermost binding as a local.
    let codes = check_with_no_unused_locals(
        r#"
export function f({
  a: {
    b: {
      c: {
        d: {
          e: {
            f: {
              g: {
                h: {
                  i: {
                    j: {
                      k: {
                        l
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}: {
  a: {
    b: {
      c: {
        d: {
          e: {
            f: {
              g: {
                h: {
                  i: {
                    j: {
                      k: {
                        l: string
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}) {}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for deeply nested destructured parameter (issue #3139). Got: {codes:?}"
    );
}

#[test]
fn deeply_nested_array_and_object_destructured_parameter_is_exempt() {
    // Mix array and object destructuring at depth — the walk must traverse
    // both ArrayBindingPattern and ObjectBindingPattern hops uniformly.
    let codes = check_with_no_unused_locals(
        r#"
export function f([
  {
    a: [
      {
        b: [
          { c: [d] }
        ]
      }
    ]
  }
]: [{ a: [{ b: [{ c: [string] }] }] }]) {}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for nested mixed array/object destructured parameter. Got: {codes:?}"
    );
}

#[test]
fn deeply_nested_local_destructuring_is_still_reported() {
    // Control: an unused local destructured variable at the same nesting depth
    // must still be reported under noUnusedLocals. The fix exempts only
    // bindings owned by a Parameter, not locals that happen to be deeply
    // nested.
    let codes = check_with_no_unused_locals(
        r#"
declare const obj: {
  a: { b: { c: { d: { e: { f: { g: { h: { i: { j: { k: { l: string } } } } } } } } } } }
};
export function f(): void {
  const {
    a: {
      b: {
        c: {
          d: {
            e: {
              f: {
                g: {
                  h: {
                    i: {
                      j: {
                        k: {
                          l
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  } = obj;
}
"#,
    );

    assert!(
        codes.contains(&6133),
        "Expected TS6133 for unused deeply nested local destructuring. Got: {codes:?}"
    );
}
