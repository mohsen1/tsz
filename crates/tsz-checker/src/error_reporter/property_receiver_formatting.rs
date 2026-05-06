pub(super) fn elide_long_property_receiver_object_literals(display: String) -> String {
    if !display.starts_with("Omit<") {
        return display;
    }

    let chars: Vec<char> = display.chars().collect();
    let mut out = String::with_capacity(display.len());
    let mut object_count = 0_u32;
    let mut idx = 0;

    while idx < chars.len() {
        if chars[idx] != '{' {
            out.push(chars[idx]);
            idx += 1;
            continue;
        }

        let start = idx;
        idx += 1;
        let mut depth = 1_i32;
        while idx < chars.len() && depth > 0 {
            match chars[idx] {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            idx += 1;
        }

        object_count += 1;
        if object_count > 3 {
            out.push_str("{ ...; }");
        } else {
            out.extend(chars[start..idx].iter());
        }
    }

    out
}
