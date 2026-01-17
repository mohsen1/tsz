pub(crate) fn push_usize(output: &mut String, value: usize) {
    push_u64(output, value as u64);
}

pub(crate) fn push_u32(output: &mut String, value: u32) {
    push_u64(output, value as u64);
}

pub(crate) fn push_i64(output: &mut String, value: i64) {
    if value < 0 {
        output.push('-');
        push_u64(output, value.wrapping_neg() as u64);
    } else {
        push_u64(output, value as u64);
    }
}

fn push_u64(output: &mut String, mut value: u64) {
    if value == 0 {
        output.push('0');
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while value > 0 {
        let digit = (value % 10) as u8;
        i -= 1;
        buf[i] = b'0' + digit;
        value /= 10;
    }

    // SAFETY: buffer only contains ASCII digits.
    let digits = unsafe { std::str::from_utf8_unchecked(&buf[i..]) };
    output.push_str(digits);
}

#[cfg(test)]
mod tests {
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
}
