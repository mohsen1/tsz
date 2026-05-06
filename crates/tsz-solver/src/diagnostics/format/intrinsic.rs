use crate::IntrinsicKind;

pub(super) const fn format_intrinsic(kind: IntrinsicKind) -> &'static str {
    match kind {
        IntrinsicKind::Any => "any",
        IntrinsicKind::Unknown => "unknown",
        IntrinsicKind::Never => "never",
        IntrinsicKind::Void => "void",
        IntrinsicKind::Null => "null",
        IntrinsicKind::Undefined => "undefined",
        IntrinsicKind::Boolean => "boolean",
        IntrinsicKind::Number => "number",
        IntrinsicKind::String => "string",
        IntrinsicKind::Bigint => "bigint",
        IntrinsicKind::Symbol => "symbol",
        IntrinsicKind::Object => "object",
        IntrinsicKind::Function => "Function",
    }
}
