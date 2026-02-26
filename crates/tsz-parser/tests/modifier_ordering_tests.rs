//! Tests for class member modifier ordering (TS1029) and ambient context (TS1040).

use crate::parser::ParserState;

fn parse_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    parse_diagnostics(source).iter().any(|(c, _)| *c == code)
}

fn count_error(source: &str, code: u32) -> usize {
    parse_diagnostics(source)
        .iter()
        .filter(|(c, _)| *c == code)
        .count()
}

// =========================================================================
// TS1029: Modifier ordering — override vs readonly
// =========================================================================

#[test]
fn override_readonly_correct_order_no_ts1029() {
    // `override readonly` is the canonical order in tsc — no TS1029
    let source = r"
class B { p: number = 1; }
class D extends B {
    override readonly p: number;
}
";
    assert!(
        !has_error(source, 1029),
        "`override readonly` should not produce TS1029 — this is the correct order"
    );
}

#[test]
fn readonly_override_wrong_order_ts1029() {
    // `readonly override` is wrong order — should produce TS1029
    let source = r"
class B { p: number = 1; }
class D extends B {
    readonly override p: number;
}
";
    assert!(
        has_error(source, 1029),
        "`readonly override` should produce TS1029 — override must precede readonly"
    );
}

#[test]
fn override_async_correct_order_no_ts1029() {
    let source = r"
class B { m(): void {} }
class D extends B {
    override async m() {}
}
";
    assert!(
        !has_error(source, 1029),
        "`override async` should not produce TS1029"
    );
}

#[test]
fn async_override_wrong_order_ts1029() {
    let source = r"
class B { m(): void {} }
class D extends B {
    async override m() {}
}
";
    assert!(
        has_error(source, 1029),
        "`async override` should produce TS1029"
    );
}

// =========================================================================
// TS1040: override in ambient context (declare)
// =========================================================================

#[test]
fn override_declare_produces_ts1040() {
    // `override declare` on a member property → TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    override declare p: number;
}
";
    assert!(
        has_error(source, 1040),
        "`override declare` should produce TS1040 — override cannot be in ambient context"
    );
}

#[test]
fn declare_override_produces_ts1040() {
    // Reverse order `declare override` also → TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    declare override p: number;
}
";
    assert!(
        has_error(source, 1040),
        "`declare override` should produce TS1040 — override cannot be in ambient context"
    );
}

#[test]
fn override_without_declare_no_ts1040() {
    // Plain override should not produce TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    override p: number;
}
";
    assert!(
        !has_error(source, 1040),
        "plain `override` should not produce TS1040"
    );
}

#[test]
fn declare_without_override_no_ts1040() {
    // Plain declare should not produce TS1040
    let source = r"
class B { p: number = 1; }
class D extends B {
    declare p: number;
}
";
    assert!(
        !has_error(source, 1040),
        "plain `declare` should not produce TS1040"
    );
}

#[test]
fn override_declare_exactly_one_ts1040() {
    // Should emit exactly one TS1040, not two
    let source = r"
class B { p: number = 1; }
class D extends B {
    override declare p: number;
}
";
    assert_eq!(
        count_error(source, 1040),
        1,
        "`override declare` should produce exactly one TS1040"
    );
}
