//! Tests for TS2347: Untyped function calls may not accept type arguments.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_bind_function_like_values_without_call_signatures_reject_type_arguments() {
    let codes = get_error_codes(
        r#"
declare var anyVar: any;
anyVar<string>("hello");
anyVar<number>();
anyVar<{}>(undefined);

interface SubFunc {
    bind(): void;
    prop: number;
}
declare var subFunc: SubFunc;
subFunc<number>(0);
subFunc<string>("");
subFunc<any>();
"#,
    );

    let count = codes.iter().filter(|&&code| code == 2347).count();
    assert_eq!(
        count, 6,
        "Should emit TS2347 for any and bind-based Function-like calls with type arguments, got: {codes:?}"
    );
}
