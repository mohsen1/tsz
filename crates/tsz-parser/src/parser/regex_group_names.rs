//! Capturing-group name scanning for regular expression literals.
//!
//! Both `(?<name>` and `\k<name>` route through the same name scan in tsc
//! (`scanGroupName`, `scanner.ts`): scan an ECMAScript identifier, report
//! `TS1514` when nothing was consumed, and — for a declaration only — report
//! `TS1515` when the name is already visible.
//!
//! "Already visible" is deliberately not a flat duplicate check. tsc keeps a
//! stack of per-alternative scopes, pushed and popped around every alternative
//! of every disjunction, and a name conflicts when it is in the current
//! alternative's scope or in any *enclosing* alternative's scope. That is what
//! makes `/(?<a>x)|(?<a>y)/` legal while `/(?<a>x)(?<a>y)/` and
//! `/(?<a>x|(?<a>y))/` are both errors.

/// Per-alternative capturing-group name scopes.
///
/// Mirrors tsc's `namedCapturingGroupsScopeStack` / `topNamedCapturingGroupsScope`
/// pair: [`Self::enter_alternative`] and [`Self::leave_alternative`] bracket
/// each alternative so sibling alternatives never see each other's names, while
/// enclosing alternatives stay visible.
#[derive(Default)]
pub(crate) struct GroupNameScopes {
    enclosing: Vec<Vec<String>>,
    current: Vec<String>,
}

impl GroupNameScopes {
    pub(crate) const fn new() -> Self {
        Self {
            enclosing: Vec::new(),
            current: Vec::new(),
        }
    }

    /// Begin one alternative of a disjunction. Names declared inside it are
    /// discarded again by [`Self::leave_alternative`], so the next alternative
    /// may reuse them.
    pub(crate) fn enter_alternative(&mut self) {
        self.enclosing.push(std::mem::take(&mut self.current));
    }

    /// End the alternative begun by [`Self::enter_alternative`].
    pub(crate) fn leave_alternative(&mut self) {
        self.current = self.enclosing.pop().unwrap_or_default();
    }

    /// Declare `name` in the current alternative's scope.
    ///
    /// Returns `false` when the name is already visible — the `TS1515` case.
    /// A conflicting name is deliberately not re-declared, matching tsc, so a
    /// third occurrence in the same scope reports against the first.
    pub(crate) fn declare(&mut self, name: &str) -> bool {
        if self.is_visible(name) {
            return false;
        }
        self.current.push(name.to_owned());
        true
    }

    fn is_visible(&self, name: &str) -> bool {
        self.current.iter().any(|declared| declared == name)
            || self
                .enclosing
                .iter()
                .any(|scope| scope.iter().any(|declared| declared == name))
    }
}

/// Scan an ECMAScript identifier at `pos`, which must sit just past the `<` of
/// a `(?<name>` or `\k<name>` form, and advance `pos` past it.
///
/// The identifier rules are the scanner's own (`ID_Start` plus `$`/`_` to
/// start, `ID_Continue` plus `$`/ZWNJ/ZWJ to continue), reached through
/// `tsz_scanner` so group names accept exactly what an identifier accepts
/// elsewhere. A `\u` escape is legal in continuation position only, which is
/// why `(?<a>x)` is a `TS1514` in tsc but `(?<ab>x)` is not.
pub(crate) fn scan_group_name(body: &[u8], end: usize, pos: &mut usize) -> Option<String> {
    let mut value = String::new();

    let (first, after_first) = next_char(body, end, *pos)?;
    if !tsz_scanner::is_ecmascript_identifier_start(first) {
        return None;
    }
    value.push(first);
    *pos = after_first;

    while let Some((ch, next)) = next_char(body, end, *pos) {
        if ch == '\\' {
            // tsc allows a `\u` escape in continuation position only, and
            // stops on a malformed one — leaving the `'>' expected.` its
            // caller then reports at the backslash.
            let Some((escaped, after_escape)) = scan_unicode_escape(body, end, *pos) else {
                break;
            };
            if !tsz_scanner::is_ecmascript_identifier_part(escaped) {
                break;
            }
            value.push(escaped);
            *pos = after_escape;
            continue;
        }

        if !tsz_scanner::is_ecmascript_identifier_part(ch) {
            break;
        }
        value.push(ch);
        *pos = next;
    }

    Some(value)
}

/// Decode a `\uHHHH` or `\u{H+}` escape starting at the backslash.
///
/// Returns the decoded character and the position just past the escape, or
/// `None` when the escape is malformed or encodes an unpaired surrogate.
fn scan_unicode_escape(body: &[u8], end: usize, backslash: usize) -> Option<(char, usize)> {
    if body.get(backslash + 1) != Some(&b'u') || backslash + 1 >= end {
        return None;
    }
    let mut pos = backslash + 2;

    if body.get(pos) == Some(&b'{') && pos < end {
        pos += 1;
        let digits_start = pos;
        let mut value: u32 = 0;
        while pos < end
            && let Some(digit) = hex_value(body[pos])
        {
            value = value.checked_mul(16)?.checked_add(digit)?;
            pos += 1;
        }
        if pos == digits_start || body.get(pos) != Some(&b'}') || pos >= end {
            return None;
        }
        return char::from_u32(value).map(|ch| (ch, pos + 1));
    }

    let mut value: u32 = 0;
    for offset in 0..4 {
        let index = pos + offset;
        if index >= end {
            return None;
        }
        value = (value << 4) | hex_value(body[index])?;
    }
    pos += 4;
    char::from_u32(value).map(|ch| (ch, pos))
}

const fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a') as u32 + 10),
        b'A'..=b'F' => Some((byte - b'A') as u32 + 10),
        _ => None,
    }
}

/// Decode the UTF-8 character at `pos`, bounded by `end`.
fn next_char(body: &[u8], end: usize, pos: usize) -> Option<(char, usize)> {
    if pos >= end {
        return None;
    }
    let width = match *body.get(pos)? {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let next = pos.checked_add(width)?;
    if next > end {
        return None;
    }
    let ch = std::str::from_utf8(body.get(pos..next)?)
        .ok()?
        .chars()
        .next()?;
    Some((ch, next))
}
