//! Tests for generic call type inference ordering:
//! non-contextual (direct) argument inference must take priority over
//! contextual (callback return type) inference.

use tsz_checker::test_utils::check_source_code_messages;

/// foo3<T,U>(x: T, cb: (a: T) => U, y: U)
/// called as foo3(1, function(a) { return ''; }, 1)
///
/// Round 1: y=1 infers U (`NakedTypeVariable` priority)
/// Round 2: callback return '' infers U (`ReturnType` priority)
///
/// tsc: U=1 from y wins, callback fails as (a:number)=>string vs (a:number)=>1
/// (error at callback position, not at y position)
#[test]
fn test_foo3_direct_arg_u_wins_over_callback_return() {
    let source = r#"
function foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
    return cb(x);
}
var r8 = foo3(1, function (a) { return ''; }, 1);
"#;
    let diags = check_source_code_messages(source);
    // One error: TS2345
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diags:?}"
    );
    let (_, msg) = ts2345[0];
    // The error should be about the callback not matching the expected signature,
    // NOT about y=1 failing against ""
    assert!(
        !msg.contains("parameter of type '\"\"'"),
        "Error should not be about y=1 vs empty string; got: {msg}",
    );
}

/// When there is NO conflicting direct argument, callback return type infers U
/// foo2<T,U>(x: T, cb: (a: T) => U)
/// called as foo2(1, (a) => '') - no error, U=string
#[test]
fn test_foo2_no_y_no_error() {
    let source = r#"
function foo2<T, U>(x: T, cb: (a: T) => U) {
    return cb(x);
}
var r4 = foo2(1, function(a) { return ''; });
"#;
    let diags = check_source_code_messages(source);
    assert!(
        diags.is_empty(),
        "expected no errors for foo2, got: {diags:?}"
    );
}
