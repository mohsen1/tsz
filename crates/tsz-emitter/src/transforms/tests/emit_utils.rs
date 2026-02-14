use super::*;

#[test]
fn push_usize_writes_digits() {
    let mut out = String::new();
    push_usize(&mut out, 0);
    out.push(',');
    push_usize(&mut out, 12345);
    assert_eq!(out, "0,12345");
}

#[test]
fn push_i64_handles_negative() {
    let mut out = String::new();
    push_i64(&mut out, -42);
    out.push(',');
    push_i64(&mut out, 7);
    assert_eq!(out, "-42,7");
}
