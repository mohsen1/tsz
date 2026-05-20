//! Private helpers for assignment literal-widening display decisions.

use tsz_solver::TypeId;

pub(super) fn simple_or_namespace_member_name(display: &str) -> Option<&str> {
    if display.starts_with("typeof ")
        || display.starts_with("import(")
        || display.contains('<')
        || display.contains('[')
        || display.contains(' ')
    {
        return None;
    }
    let name = display.rsplit_once('.').map_or(display, |(_, short)| short);
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return None;
    }
    chars
        .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        .then_some(name)
}

/// Whether `target` accepts a literal whose widened primitive kind is
/// `source_primitive` for "literal-of-contextual-type" purposes.
pub(super) fn target_accepts_literal_primitive_kind(
    db: &dyn tsz_solver::construction::TypeDatabase,
    target: TypeId,
    source_primitive: TypeId,
) -> bool {
    target_accepts_literal_primitive_kind_inner(db, target, source_primitive, 0)
}

fn target_accepts_literal_primitive_kind_inner(
    db: &dyn tsz_solver::construction::TypeDatabase,
    target: TypeId,
    source_primitive: TypeId,
    depth: u32,
) -> bool {
    use crate::query_boundaries::common;
    if depth > 32 {
        return true;
    }
    if let Some(members) = common::union_members(db, target) {
        return members.iter().any(|&m| {
            target_accepts_literal_primitive_kind_inner(db, m, source_primitive, depth + 1)
        });
    }
    if let Some(members) = common::intersection_members(db, target) {
        return members.iter().any(|&m| {
            target_accepts_literal_primitive_kind_inner(db, m, source_primitive, depth + 1)
        });
    }
    if let Some(value) = common::literal_value(db, target) {
        return value.primitive_type_id() == source_primitive;
    }
    if source_primitive == TypeId::STRING
        && (common::is_template_literal_type(db, target)
            || common::is_string_intrinsic_type(db, target))
    {
        return true;
    }
    if target == TypeId::UNDEFINED || target == TypeId::NULL {
        return true;
    }
    if target == TypeId::NEVER {
        return true;
    }
    true
}

/// Mirror tsc's literal display policy for TS2322 messages against
/// `undefined` / `null` targets.
pub(super) fn literal_display_appropriate_for_undefined_null_target(
    db: &dyn tsz_solver::construction::TypeDatabase,
    target: TypeId,
    display: &str,
) -> bool {
    use crate::query_boundaries::common;
    let target = common::evaluate_type(db, target);
    if target != TypeId::UNDEFINED && target != TypeId::NULL {
        return true;
    }
    if matches!(display, "true" | "false") {
        return true;
    }
    let trimmed = display.trim();
    if trimmed.is_empty() {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
        return true;
    }
    if first == b'`' && last == b'`' {
        return true;
    }
    let numeric_start = first == b'-' || first == b'+' || first == b'.' || first.is_ascii_digit();
    if numeric_start
        && trimmed.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+' || c == '_' || c == 'n'
        })
    {
        return true;
    }
    false
}
