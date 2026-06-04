fn iterator_protocol_next_type_arg(display: &str) -> Option<&str> {
    if let Some((name, args)) = parse_simple_type_application_display(display)
        && matches!(
            name,
            "Generator" | "Iterator" | "IteratorObject" | "Iterable"
        )
    {
        return args.get(2).copied();
    }

    display
        .split_once("next(..._: [] | [")
        .and_then(|(_, rest)| rest.split_once("])").map(|(next, _)| next.trim()))
}

fn function_return_display(display: &str) -> Option<&str> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut iter = display.char_indices().peekable();

    while let Some((idx, ch)) = iter.next() {
        if ch == '=' && iter.peek().is_some_and(|(_, next)| *next == '>') {
            if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 && angle_depth == 0 {
                return display.get(idx + 2..).map(str::trim);
            }
            iter.next();
            continue;
        }

        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' if angle_depth > 0 => angle_depth -= 1,
            _ => {}
        }
    }

    None
}

fn iterator_next_type_accepts(source_next: &str, target_next: &str) -> bool {
    source_next == target_next
        || source_next == "any"
        || target_next == "any"
        || source_next == "unknown"
        || target_next == "never"
}
