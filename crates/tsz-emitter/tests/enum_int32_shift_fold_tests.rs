//! Constant folding of bitwise operators in numeric enum initializers must use
//! ECMAScript int32 semantics.
//!
//! Structural rule: when a numeric enum member initializer is a constant
//! bitwise expression (`<<`, `>>`, `>>>`, `&`, `|`, `^`, `~`), tsc coerces the
//! operands to int32, masks the shift count to its low 5 bits, sign-extends for
//! `<<`/`>>`, and zero-fills for `>>>`. Numeric literals (including hex/binary
//! /octal forms and digit separators) are parsed to their integer value first.
//! This change makes tsz fold the same values rather than performing a raw i64
//! shift on the source spelling.
//!
//! Tests vary member names, bases, and shift counts so they pin the int32 rule
//! rather than a single rendered fingerprint.

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn emit_es2015(source: &str) -> String {
    parse_lower_emit(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

fn assert_member(output: &str, enum_name: &str, member: &str, value: &str) {
    let needle = format!(r#"{enum_name}[{enum_name}["{member}"] = {value}] = "{member}";"#);
    assert!(
        output.contains(&needle),
        "expected `{needle}` in output.\nOutput:\n{output}"
    );
}

/// Left-shift folding masks the shift count to 5 bits and sign-extends the i32
/// result. `1 << 32` masks to `1 << 0` = 1; `1 << -1` masks to `1 << 31`, which
/// as a signed int32 is the minimum value.
#[test]
fn left_shift_masks_count_and_sign_extends() {
    let output = emit_es2015(
        r#"
enum Shl {
    A = 1 << 1,
    B = 1 << 32,
    C = 1 << 123,
    E = 1 << -1,
    G = 1 << -123,
}
"#,
    );
    assert_member(&output, "Shl", "A", "2");
    assert_member(&output, "Shl", "B", "1");
    assert_member(&output, "Shl", "C", "134217728");
    assert_member(&output, "Shl", "E", "-2147483648");
    assert_member(&output, "Shl", "G", "32");
}

/// A signed right shift coerces the left operand to int32 first, so a literal
/// whose value exceeds i32 range (`0xFFFFFFFF` = all bits set) is treated as
/// `-1`. Renaming the member or the enum does not change the folded value.
#[test]
fn signed_right_shift_uses_int32_left_operand() {
    let output = emit_es2015(
        r#"
enum Sar {
    First = 0xFF_FF_FF_FF >> 1,
    Second = 0xFFFFFFFF >> 32,
    Third = 0xFFFFFFFF >> -1,
}
"#,
    );
    // `-1 >> n` is `-1` for every masked count.
    assert_member(&output, "Sar", "First", "-1");
    assert_member(&output, "Sar", "Second", "-1");
    assert_member(&output, "Sar", "Third", "-1");
}

/// An unsigned right shift coerces the left operand to a u32, so `0xFFFFFFFF`
/// (parsed from a digit-separated hex literal) behaves as `4294967295`.
#[test]
fn unsigned_right_shift_is_zero_fill() {
    let output = emit_es2015(
        r#"
enum Shr {
    Lo = 0xFF_FF_FF_FF >>> 1,
    Zero = 0xFFFFFFFF >>> 32,
    Hi = 0xFFFFFFFF >>> 123,
}
"#,
    );
    assert_member(&output, "Shr", "Lo", "2147483647");
    assert_member(&output, "Shr", "Zero", "4294967295");
    assert_member(&output, "Shr", "Hi", "31");
}

/// Hex/binary/octal numeric literals (and separators) parse to their integer
/// value before folding, and bitwise AND/OR/XOR operate on int32 operands.
#[test]
fn radix_literals_parse_and_bitwise_ops_are_int32() {
    let output = emit_es2015(
        r#"
enum Bits {
    Hex = 0xFF & 0x0F,
    Bin = 0b1010 | 0b0101,
    Xor = 0xFFFFFFFF ^ 0,
    Sep = 1_000_000 + 1,
}
"#,
    );
    assert_member(&output, "Bits", "Hex", "15");
    assert_member(&output, "Bits", "Bin", "15");
    // `0xFFFFFFFF ^ 0` coerces to int32 -> -1.
    assert_member(&output, "Bits", "Xor", "-1");
    assert_member(&output, "Bits", "Sep", "1000001");
}
