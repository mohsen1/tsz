use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::{Expression, ExpressionKind, Literal, Token, TokenKind};

macro_rules! string_field_accessors {
    ($($field:ident),+ $(,)?) => {$(
        #[must_use]
        pub fn $field(&self) -> &str {
            &self.$field
        }
    )+};
}

/// Syntax-owned spelling for an ordinary or scanner-recovered number token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberLiteral {
    Plain(String),
    Separated(SeparatedNumberLiteral),
    Recovery(NumericRecoveryLiteral),
}

impl NumberLiteral {
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Separated(literal) => &literal.raw,
            Self::Recovery(literal) => &literal.raw,
        }
    }

    /// A syntactically valid spelling with the same numeric value as the
    /// recovered token. Malformed exponent text never reaches the type store.
    #[must_use]
    pub fn semantic_text(&self) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Separated(literal) => &literal.raw,
            Self::Recovery(literal) => &literal.semantic_text,
        }
    }

    /// JavaScript spelling selected from scanner-owned syntax metadata.
    #[must_use]
    pub fn emit_text(&self, preserve_separators: bool) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Separated(literal) if preserve_separators => &literal.raw,
            Self::Separated(literal) => &literal.canonical,
            Self::Recovery(literal) => &literal.emit_text,
        }
    }

    /// TypeScript adds a second property-access dot only for downleveled
    /// decimal spellings that canonicalize to an integer. Radix specifiers
    /// deliberately suppress this rule in the upstream printer.
    #[must_use]
    pub fn needs_property_access_extra_dot(&self, preserve_separators: bool) -> bool {
        let Self::Separated(literal) = self else {
            return false;
        };
        let selected = self.emit_text(preserve_separators);
        !literal.with_radix_specifier
            && !selected
                .bytes()
                .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
    }

    #[must_use]
    pub const fn validation_supported(&self) -> bool {
        match self {
            Self::Plain(_) | Self::Separated(_) => true,
            Self::Recovery(literal) => literal.validation_supported,
        }
    }

    pub(crate) const fn recovery_kind(&self) -> Option<NumericRecoveryKind> {
        match self {
            Self::Plain(_) | Self::Separated(_) => None,
            Self::Recovery(literal) => Some(literal.kind),
        }
    }
}

/// A valid Number token containing numeric separators. Raw spelling is kept
/// for ES2021+ source preservation; canonical text is the scanner-observed
/// JavaScript Number value used by downlevel emit and synthesized products.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeparatedNumberLiteral {
    raw: String,
    canonical: String,
    with_radix_specifier: bool,
}

impl SeparatedNumberLiteral {
    fn from_valid_source(raw: &str, with_radix_specifier: bool) -> Option<Self> {
        let parsed = parse_number_literal(raw)?;
        Some(Self {
            raw: raw.to_string(),
            canonical: parsed.display,
            with_radix_specifier,
        })
    }

    string_field_accessors!(raw, canonical);
}

/// Canonical JavaScript Number value produced from source spelling. Scanner
/// metadata and semantic literal identity share this parser, while each owner
/// retains its own typed representation.
#[derive(Debug, Clone)]
pub(crate) struct ParsedNumberLiteral {
    pub(crate) value: f64,
    pub(crate) display: String,
}

pub(crate) fn parse_number_literal(source: &str) -> Option<ParsedNumberLiteral> {
    let compact = source.replace('_', "");
    let value = if let Some((digits, radix)) = prefixed_numeric(&compact) {
        parse_power_of_two_integer(digits, radix.ilog2() as usize)?
    } else if compact.len() > 1
        && compact.starts_with('0')
        && compact.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
    {
        parse_power_of_two_integer(&compact[1..], 3)?
    } else {
        if !is_decimal_literal(&compact) {
            return None;
        }
        compact.parse::<f64>().ok()?
    };
    let value = if value == 0.0 { 0.0 } else { value };
    Some(ParsedNumberLiteral {
        value,
        display: javascript_number_to_string(value),
    })
}

pub(crate) fn erased_expression_separated_number(
    expression: &Expression,
) -> Option<&NumberLiteral> {
    let expression = erased_assertion_expression(expression).unwrap_or(expression);
    match &expression.kind {
        ExpressionKind::Literal(Literal::Number(number @ NumberLiteral::Separated(_))) => {
            Some(number)
        }
        _ => None,
    }
}

/// Return the JavaScript expression left after erasing an assertion and every
/// parenthesis layer whose only purpose is to contain that assertion.
pub(crate) fn erased_assertion_expression(expression: &Expression) -> Option<&Expression> {
    match &expression.kind {
        ExpressionKind::As { expression, .. } => {
            Some(erased_assertion_expression(expression).unwrap_or(expression))
        }
        ExpressionKind::Parenthesized(inner) => erased_assertion_expression(inner),
        _ => None,
    }
}

/// Scanner-owned numeric recovery. The three spellings stay separate so emit
/// never derives JavaScript text from a checker display or a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericRecoveryLiteral {
    raw: String,
    semantic_text: String,
    emit_text: String,
    kind: NumericRecoveryKind,
    validation_supported: bool,
}

impl NumericRecoveryLiteral {
    string_field_accessors!(raw, semantic_text, emit_text);

    #[must_use]
    pub const fn validation_supported(&self) -> bool {
        self.validation_supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericRecoveryKind {
    LegacyOctal,
    LeadingZeroDecimal,
    MissingExponentDigits,
    InvalidSeparator,
    IncompleteRadix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedNumericLiteral {
    pub span: Span,
    literal: NumericRecoveryLiteral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedSeparatedNumberLiteral {
    pub span: Span,
    literal: SeparatedNumberLiteral,
}

impl ScannedSeparatedNumberLiteral {
    pub(super) fn from_valid_token(
        source: &SourceText,
        span: Span,
        with_radix_specifier: bool,
    ) -> Option<Self> {
        let raw = source.slice(span);
        raw.contains('_').then_some(())?;
        Some(Self {
            span,
            literal: SeparatedNumberLiteral::from_valid_source(raw, with_radix_specifier)?,
        })
    }

    pub(super) fn syntax_literal(&self) -> SeparatedNumberLiteral {
        self.literal.clone()
    }
}

impl ScannedNumericLiteral {
    pub(super) fn syntax_literal(&self) -> NumericRecoveryLiteral {
        self.literal.clone()
    }
}

pub(super) struct ScannedNumericToken {
    pub end: usize,
    pub kind: TokenKind,
    pub diagnostics: Vec<Diagnostic>,
    pub recovery_literal: Option<ScannedNumericLiteral>,
    pub separated_literal: Option<ScannedSeparatedNumberLiteral>,
    pub has_unmodeled_separator: bool,
}

pub(super) fn scan_numeric_literal(
    source: &SourceText,
    start: usize,
    previous_token: Option<Token>,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let mut offset = start;
    let starts_with_dot = bytes.get(offset) == Some(&b'.');
    if starts_with_dot {
        offset += 1;
        let fraction_run = consume_digits(bytes, &mut offset, 10);
        let integer = &source.text[start..offset];
        finish_decimal_numeric(
            source,
            start,
            offset,
            integer,
            false,
            true,
            true,
            fraction_run.invalid_separator,
            fraction_run.saw_separator,
        )
    } else if let Some((_, radix)) = prefixed_numeric(&source.text[offset..]) {
        offset += 2;
        scan_prefixed_numeric(source, start, offset, radix)
    } else {
        let leading_zero = bytes.get(start) == Some(&b'0');
        let mut invalid_separator = false;
        let mut saw_separator = false;
        if leading_zero {
            consume_ascii_digits(bytes, &mut offset);
            if offset == start + 1 && bytes.get(offset) == Some(&b'_') {
                let run = consume_digits(bytes, &mut offset, 10);
                invalid_separator = run.invalid_separator;
                saw_separator = run.saw_separator;
            }
        } else {
            let run = consume_digits(bytes, &mut offset, 10);
            invalid_separator = run.invalid_separator;
            saw_separator = run.saw_separator;
        }

        let integer_end = offset;
        let integer = &source.text[start..integer_end];
        let has_legacy_prefix = integer.len() > 1 && leading_zero && !integer.contains('_');
        if has_legacy_prefix && integer.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
            let negative = previous_token.is_some_and(|token| token.kind == TokenKind::Minus);
            let replacement_digits = legacy_octal_replacement_digits(integer);
            let replacement = if negative {
                format!("-0o{replacement_digits}")
            } else {
                format!("0o{replacement_digits}")
            };
            let mut diagnostic = Diagnostic::at(
                source,
                Span::new(source.id, start, integer_end),
                format!("Octal literals are not allowed. Use the syntax '{replacement}'."),
                1121,
            );
            if negative {
                diagnostic.start = diagnostic.start.saturating_sub(1);
                diagnostic.length = diagnostic.length.saturating_add(1);
            }
            let canonical_text = canonical_bounded_integer(integer, 8);
            let recovered_text = canonical_text
                .clone()
                .unwrap_or_else(|| integer.to_string());
            return recovered_numeric_token_with_kind(
                source,
                start,
                integer_end,
                recovered_text.clone(),
                recovered_text,
                NumericRecoveryKind::LegacyOctal,
                canonical_text.is_some(),
                vec![diagnostic],
                TokenKind::NumericLiteral,
            );
        }

        let leading_zero_decimal = has_legacy_prefix;
        let mut has_fraction_or_exponent = false;
        let mut mantissa_has_fraction = false;
        if bytes.get(offset) == Some(&b'.') {
            has_fraction_or_exponent = true;
            mantissa_has_fraction = true;
            offset += 1;
            let run = consume_digits(bytes, &mut offset, 10);
            invalid_separator |= run.invalid_separator;
            saw_separator |= run.saw_separator;
        }

        finish_decimal_numeric(
            source,
            start,
            offset,
            integer,
            leading_zero_decimal,
            has_fraction_or_exponent,
            mantissa_has_fraction,
            invalid_separator,
            saw_separator,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_decimal_numeric(
    source: &SourceText,
    start: usize,
    mut offset: usize,
    integer: &str,
    leading_zero_decimal: bool,
    mut has_fraction_or_exponent: bool,
    mantissa_has_fraction: bool,
    mut invalid_separator: bool,
    mut saw_separator: bool,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let mut missing_exponent_digits = false;
    let mut semantic_end = offset;
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        has_fraction_or_exponent = true;
        semantic_end = offset;
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        let exponent_run = consume_digits(bytes, &mut offset, 10);
        missing_exponent_digits = exponent_run.digit_count == 0;
        invalid_separator |= exponent_run.invalid_separator;
        saw_separator |= exponent_run.saw_separator;
    }

    if !has_fraction_or_exponent && !leading_zero_decimal && !invalid_separator {
        return plain_numeric_token(source, start, bytes, offset, saw_separator, false);
    }

    let mut diagnostics = Vec::new();
    if leading_zero_decimal {
        diagnostics.push(Diagnostic::at(
            source,
            Span::new(source.id, start, offset),
            "Decimals with leading zeros are not allowed.".to_string(),
            1489,
        ));
    }
    if missing_exponent_digits {
        diagnostics.push(Diagnostic::at(
            source,
            Span::new(source.id, offset, offset),
            "Digit expected.".to_string(),
            1124,
        ));
    }
    if diagnostics.is_empty() && !invalid_separator {
        return plain_numeric_token(source, start, bytes, offset, saw_separator, false);
    }

    let missing_exponent_suffix = missing_exponent_digits
        && !invalid_separator
        && bytes.get(offset) == Some(&b'n')
        && !conservatively_continues_identifier(bytes, offset + 1);
    let token_kind =
        if invalid_separator && !has_fraction_or_exponent && bytes.get(offset) == Some(&b'n') {
            offset += 1;
            TokenKind::BigIntLiteral
        } else {
            if missing_exponent_suffix {
                offset += 1;
            }
            TokenKind::NumericLiteral
        };

    let raw = source.text[start..offset].to_string();
    let (semantic_text, emit_text, kind, validation_supported) = if missing_exponent_digits {
        let semantic_text = source.text[start..semantic_end].replace('_', "");
        (
            semantic_text,
            raw,
            NumericRecoveryKind::MissingExponentDigits,
            !leading_zero_decimal
                && !mantissa_has_fraction
                && !invalid_separator
                && !missing_exponent_suffix,
        )
    } else if invalid_separator && !leading_zero_decimal {
        (
            raw.replace('_', ""),
            raw.clone(),
            NumericRecoveryKind::InvalidSeparator,
            false,
        )
    } else {
        let canonical = (!has_fraction_or_exponent)
            .then(|| canonical_bounded_integer(integer, 10))
            .flatten();
        (
            canonical.clone().unwrap_or_else(|| raw.replace('_', "")),
            canonical.clone().unwrap_or_else(|| raw.clone()),
            NumericRecoveryKind::LeadingZeroDecimal,
            canonical.is_some(),
        )
    };
    recovered_numeric_token_with_kind(
        source,
        start,
        offset,
        semantic_text,
        emit_text,
        kind,
        validation_supported,
        diagnostics,
        token_kind,
    )
}

#[allow(clippy::too_many_arguments)]
fn recovered_numeric_token_with_kind(
    source: &SourceText,
    start: usize,
    end: usize,
    semantic_text: String,
    emit_text: String,
    kind: NumericRecoveryKind,
    validation_supported: bool,
    diagnostics: Vec<Diagnostic>,
    token_kind: TokenKind,
) -> ScannedNumericToken {
    let raw = source.text[start..end].to_string();
    let has_unmodeled_separator = raw.contains('_')
        && matches!(
            kind,
            NumericRecoveryKind::InvalidSeparator | NumericRecoveryKind::IncompleteRadix
        );
    ScannedNumericToken {
        end,
        kind: token_kind,
        recovery_literal: Some(ScannedNumericLiteral {
            span: Span::new(source.id, start, end),
            literal: NumericRecoveryLiteral {
                raw,
                semantic_text,
                emit_text,
                kind,
                validation_supported,
            },
        }),
        diagnostics,
        separated_literal: None,
        has_unmodeled_separator,
    }
}

fn scan_prefixed_numeric(
    source: &SourceText,
    start: usize,
    mut offset: usize,
    radix: u32,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let run = consume_digits(bytes, &mut offset, radix);
    if run.digit_count == 0 && !run.invalid_separator {
        let token_kind = consume_numeric_suffix(bytes, &mut offset);
        return recovered_numeric_token_with_kind(
            source,
            start,
            offset,
            "0".to_string(),
            source.text[start..offset].to_string(),
            NumericRecoveryKind::IncompleteRadix,
            false,
            Vec::new(),
            token_kind,
        );
    }
    let plain = plain_numeric_token(source, start, bytes, offset, run.saw_separator, true);
    if !run.invalid_separator {
        return plain;
    }
    let kind = if run.digit_count == 0 {
        NumericRecoveryKind::IncompleteRadix
    } else {
        NumericRecoveryKind::InvalidSeparator
    };
    let semantic_text = if run.digit_count == 0 || plain.kind == TokenKind::BigIntLiteral {
        "0".to_string()
    } else {
        source.text[start..plain.end].replace('_', "")
    };
    recovered_numeric_token_with_kind(
        source,
        start,
        plain.end,
        semantic_text,
        source.text[start..plain.end].to_string(),
        kind,
        false,
        Vec::new(),
        plain.kind,
    )
}

fn plain_numeric_token(
    source: &SourceText,
    start: usize,
    bytes: &[u8],
    mut offset: usize,
    saw_separator: bool,
    with_radix_specifier: bool,
) -> ScannedNumericToken {
    let kind = consume_numeric_suffix(bytes, &mut offset);
    let span = Span::new(source.id, start, offset);
    let separated_literal = (kind == TokenKind::NumericLiteral && saw_separator)
        .then(|| {
            ScannedSeparatedNumberLiteral::from_valid_token(source, span, with_radix_specifier)
        })
        .flatten();
    let has_unmodeled_separator = saw_separator
        && (kind == TokenKind::BigIntLiteral
            || separated_literal.is_none()
            || conservatively_continues_identifier(bytes, offset));
    ScannedNumericToken {
        end: offset,
        kind,
        diagnostics: Vec::new(),
        recovery_literal: None,
        has_unmodeled_separator,
        separated_literal,
    }
}

#[derive(Debug, Clone, Copy)]
struct DigitRun {
    digit_count: usize,
    invalid_separator: bool,
    saw_separator: bool,
}

fn consume_ascii_digits(bytes: &[u8], offset: &mut usize) -> usize {
    let start = *offset;
    while bytes.get(*offset).is_some_and(u8::is_ascii_digit) {
        *offset += 1;
    }
    *offset - start
}

fn consume_numeric_suffix(bytes: &[u8], offset: &mut usize) -> TokenKind {
    if bytes.get(*offset) == Some(&b'n') {
        *offset += 1;
        TokenKind::BigIntLiteral
    } else {
        TokenKind::NumericLiteral
    }
}

fn conservatively_continues_identifier(bytes: &[u8], offset: usize) -> bool {
    bytes.get(offset).is_some_and(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'\\') || *byte >= 0x80
    })
}

fn consume_digits(bytes: &[u8], offset: &mut usize, radix: u32) -> DigitRun {
    let mut digit_count = 0;
    let mut saw_separator = false;
    let mut previous_was_digit = false;
    let mut invalid_separator = false;
    while bytes.get(*offset).is_some_and(|byte| {
        *byte == b'_'
            || matches!(radix, 2 | 8 | 10 | 16)
                && radix_digit(*byte).is_some_and(|digit| u32::from(digit) < radix)
    }) {
        if bytes[*offset] == b'_' {
            saw_separator = true;
            invalid_separator |= !previous_was_digit;
            previous_was_digit = false;
        } else {
            digit_count += 1;
            previous_was_digit = true;
        }
        *offset += 1;
    }
    invalid_separator |= saw_separator && !previous_was_digit;
    DigitRun {
        digit_count,
        invalid_separator,
        saw_separator,
    }
}

fn is_decimal_literal(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut position = 0;
    let mut mantissa_digits = 0;

    mantissa_digits += consume_ascii_digits(bytes, &mut position);
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        mantissa_digits += consume_ascii_digits(bytes, &mut position);
    }
    if mantissa_digits == 0 {
        return false;
    }
    if matches!(bytes.get(position), Some(b'e' | b'E')) {
        position += 1;
        if matches!(bytes.get(position), Some(b'+' | b'-')) {
            position += 1;
        }
        let exponent_start = position;
        consume_ascii_digits(bytes, &mut position);
        if position == exponent_start {
            return false;
        }
    }
    position == bytes.len()
}

/// Parse a binary, octal, or hexadecimal integer directly into the correctly
/// rounded JavaScript Number. These radices are powers of two, so retaining the
/// leading 53 bits plus guard/sticky bits is exact even when the token is wider
/// than Rust's integer types.
fn parse_power_of_two_integer(digits: &str, bits_per_digit: usize) -> Option<f64> {
    if digits.is_empty() {
        return None;
    }
    let radix = 1_u8 << bits_per_digit;
    let mut first_nonzero = None;
    for (index, byte) in digits.bytes().enumerate() {
        let value = radix_digit(byte)?;
        if value >= radix {
            return None;
        }
        if value != 0 && first_nonzero.is_none() {
            first_nonzero = Some((index, value));
        }
    }
    let Some((first_index, first_value)) = first_nonzero else {
        return Some(0.0);
    };

    let first_width = (u8::BITS - first_value.leading_zeros()) as usize;
    let trailing_digits = digits.len() - first_index - 1;
    let bit_length = trailing_digits
        .checked_mul(bits_per_digit)
        .and_then(|width| width.checked_add(first_width))
        .unwrap_or(usize::MAX);
    if bit_length > 1024 {
        return Some(f64::INFINITY);
    }

    let mut leading = 0_u64;
    let mut consumed = 0_usize;
    let mut guard = false;
    let mut sticky = false;
    for (relative_index, byte) in digits.as_bytes()[first_index..].iter().enumerate() {
        let value = radix_digit(*byte)?;
        let width = if relative_index == 0 {
            first_width
        } else {
            bits_per_digit
        };
        for bit_index in (0..width).rev() {
            let bit = (value >> bit_index) & 1;
            if consumed < 53 {
                leading = (leading << 1) | u64::from(bit);
            } else if consumed == 53 {
                guard = bit != 0;
            } else {
                sticky |= bit != 0;
            }
            consumed += 1;
        }
    }

    if bit_length <= 53 {
        return Some(leading as f64);
    }
    if guard && (sticky || leading & 1 != 0) {
        leading += 1;
    }

    let mut exponent = bit_length - 1;
    if leading == 1_u64 << 53 {
        leading >>= 1;
        exponent += 1;
    }
    if exponent > 1023 {
        return Some(f64::INFINITY);
    }
    let fraction = leading & ((1_u64 << 52) - 1);
    Some(f64::from_bits(
        (((exponent + 1023) as u64) << 52) | fraction,
    ))
}

const fn radix_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn prefixed_numeric(source: &str) -> Option<(&str, u32)> {
    match source.as_bytes().get(..2)? {
        [b'0', b'x' | b'X'] => Some((&source[2..], 16)),
        [b'0', b'b' | b'B'] => Some((&source[2..], 2)),
        [b'0', b'o' | b'O'] => Some((&source[2..], 8)),
        _ => None,
    }
}

/// ECMAScript's Number-to-string thresholds differ from Rust's Display
/// thresholds. Rust's shortest-roundtrip digits are reused, then placed in
/// fixed or exponential notation at the JavaScript boundaries.
fn javascript_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value.is_infinite() {
        return "Infinity".to_string();
    }

    let shortest = format!("{value:?}");
    let (mantissa, explicit_exponent) = shortest
        .split_once(['e', 'E'])
        .map_or((shortest.as_str(), None), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().ok())
        });
    let mut digits: String = mantissa
        .bytes()
        .filter(|byte| *byte != b'.')
        .map(char::from)
        .collect();
    let significant_start = digits
        .bytes()
        .position(|byte| byte != b'0')
        .expect("a nonzero finite number has a nonzero decimal digit");
    digits.drain(..significant_start);
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }

    let scientific_exponent = explicit_exponent.unwrap_or_else(|| {
        if let Some(dot) = mantissa.find('.') {
            if !mantissa.starts_with('0') {
                dot as i32 - 1
            } else {
                let first_nonzero = mantissa
                    .bytes()
                    .position(|byte| byte != b'0' && byte != b'.')
                    .expect("a nonzero finite number has a nonzero decimal digit");
                -(first_nonzero as i32 - 1)
            }
        } else {
            mantissa.len() as i32 - 1
        }
    });

    if (-6..21).contains(&scientific_exponent) {
        let decimal_position = scientific_exponent + 1;
        if decimal_position <= 0 {
            return format!("0.{}{}", "0".repeat((-decimal_position) as usize), digits);
        }
        let decimal_position = decimal_position as usize;
        if decimal_position >= digits.len() {
            let trailing_zeroes = decimal_position - digits.len();
            return format!("{}{}", digits, "0".repeat(trailing_zeroes));
        }
        return format!(
            "{}.{}",
            &digits[..decimal_position],
            &digits[decimal_position..]
        );
    }

    let sign = if scientific_exponent >= 0 { "+" } else { "" };
    if digits.len() == 1 {
        format!("{digits}e{sign}{scientific_exponent}")
    } else {
        format!(
            "{}.{}e{sign}{scientific_exponent}",
            &digits[..1],
            &digits[1..]
        )
    }
}

fn canonical_bounded_integer(digits: &str, radix: u32) -> Option<String> {
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    let value = u64::from_str_radix(digits, radix).ok()?;
    (value <= MAX_SAFE_INTEGER).then(|| value.to_string())
}

fn legacy_octal_replacement_digits(digits: &str) -> String {
    const MAX_SIGNED_OCTAL: &str = "777777777777777777777";
    let value = u64::from_str_radix(digits, 8).ok();
    if value.is_none_or(|value| value > i64::MAX as u64) {
        return MAX_SIGNED_OCTAL.to_string();
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".to_string()
    } else {
        digits.to_string()
    }
}
