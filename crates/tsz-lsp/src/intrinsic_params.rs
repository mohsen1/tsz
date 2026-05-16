//! Display-only parameter specs for primitive intrinsic methods.
//!
//! Used by signature help to show real parameter names (e.g. `pos`, `searchString`) instead
//! of the no-lib fallback shape `...args: any[]` produced by `make_apparent_method_type`.
//! Does not affect type-checking behaviour.

use tsz_solver::{IntrinsicKind, TypeId};

/// Type hint for a parameter in an intrinsic method signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicParamTypeHint {
    String,
    Number,
    Boolean,
    Any,
}

impl IntrinsicParamTypeHint {
    pub const fn to_type_id(self) -> TypeId {
        match self {
            Self::String => TypeId::STRING,
            Self::Number => TypeId::NUMBER,
            Self::Boolean => TypeId::BOOLEAN,
            Self::Any => TypeId::ANY,
        }
    }
}

/// One parameter of an intrinsic primitive method for LSP display.
#[derive(Clone, Copy, Debug)]
pub struct IntrinsicParamSpec {
    pub name: &'static str,
    pub ty: IntrinsicParamTypeHint,
    pub optional: bool,
    pub rest: bool,
}

static PARAMS_SEARCH_AND_POS: [IntrinsicParamSpec; 2] = [
    IntrinsicParamSpec {
        name: "searchString",
        ty: IntrinsicParamTypeHint::String,
        optional: false,
        rest: false,
    },
    IntrinsicParamSpec {
        name: "position",
        ty: IntrinsicParamTypeHint::Number,
        optional: true,
        rest: false,
    },
];
static PARAMS_SEARCH_AND_END_POS: [IntrinsicParamSpec; 2] = [
    IntrinsicParamSpec {
        name: "searchString",
        ty: IntrinsicParamTypeHint::String,
        optional: false,
        rest: false,
    },
    IntrinsicParamSpec {
        name: "endPosition",
        ty: IntrinsicParamTypeHint::Number,
        optional: true,
        rest: false,
    },
];
static PARAMS_SLICE: [IntrinsicParamSpec; 2] = [
    IntrinsicParamSpec {
        name: "start",
        ty: IntrinsicParamTypeHint::Number,
        optional: true,
        rest: false,
    },
    IntrinsicParamSpec {
        name: "end",
        ty: IntrinsicParamTypeHint::Number,
        optional: true,
        rest: false,
    },
];
static PARAMS_SUBSTRING: [IntrinsicParamSpec; 2] = [
    IntrinsicParamSpec {
        name: "start",
        ty: IntrinsicParamTypeHint::Number,
        optional: false,
        rest: false,
    },
    IntrinsicParamSpec {
        name: "end",
        ty: IntrinsicParamTypeHint::Number,
        optional: true,
        rest: false,
    },
];
static PARAMS_PAD: [IntrinsicParamSpec; 2] = [
    IntrinsicParamSpec {
        name: "maxLength",
        ty: IntrinsicParamTypeHint::Number,
        optional: false,
        rest: false,
    },
    IntrinsicParamSpec {
        name: "fillString",
        ty: IntrinsicParamTypeHint::String,
        optional: true,
        rest: false,
    },
];
static PARAMS_CONCAT_STRINGS: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "strings",
    ty: IntrinsicParamTypeHint::String,
    optional: false,
    rest: true,
}];
static PARAMS_SINGLE_POS: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "pos",
    ty: IntrinsicParamTypeHint::Number,
    optional: false,
    rest: false,
}];
static PARAMS_SINGLE_INDEX: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "index",
    ty: IntrinsicParamTypeHint::Number,
    optional: false,
    rest: false,
}];
static PARAMS_SINGLE_COUNT: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "count",
    ty: IntrinsicParamTypeHint::Number,
    optional: false,
    rest: false,
}];
static PARAMS_OPT_FORM: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "form",
    ty: IntrinsicParamTypeHint::String,
    optional: true,
    rest: false,
}];
static PARAMS_THAT_STRING: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "that",
    ty: IntrinsicParamTypeHint::String,
    optional: false,
    rest: false,
}];
static PARAMS_OPT_DIGITS: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "digits",
    ty: IntrinsicParamTypeHint::Number,
    optional: true,
    rest: false,
}];
static PARAMS_OPT_FRACTION_DIGITS: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "fractionDigits",
    ty: IntrinsicParamTypeHint::Number,
    optional: true,
    rest: false,
}];
static PARAMS_OPT_PRECISION: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "precision",
    ty: IntrinsicParamTypeHint::Number,
    optional: true,
    rest: false,
}];
static PARAMS_OPT_RADIX: [IntrinsicParamSpec; 1] = [IntrinsicParamSpec {
    name: "radix",
    ty: IntrinsicParamTypeHint::Number,
    optional: true,
    rest: false,
}];

/// Return the actual parameter specs for a named intrinsic primitive method.
///
/// Returns `None` for methods whose parameter shapes are too complex to represent
/// here (e.g. `replace`, `split`, which take `RegExp` overloads).  The caller
/// falls back to the synthetic `...args: any[]` shape in that case.
pub fn intrinsic_method_params(
    kind: IntrinsicKind,
    method: &str,
) -> Option<&'static [IntrinsicParamSpec]> {
    match kind {
        IntrinsicKind::String => Some(match method {
            "toLowerCase" | "toUpperCase" | "trim" | "toString" | "valueOf" | "trimStart"
            | "trimEnd" | "trimLeft" | "trimRight" | "toLocaleLowerCase" | "toLocaleUpperCase"
            | "toWellFormed" => &[],
            "charAt" | "codePointAt" => &PARAMS_SINGLE_POS,
            "charCodeAt" => &PARAMS_SINGLE_INDEX,
            "repeat" => &PARAMS_SINGLE_COUNT,
            "normalize" => &PARAMS_OPT_FORM,
            "localeCompare" => &PARAMS_THAT_STRING,
            "concat" => &PARAMS_CONCAT_STRINGS,
            "indexOf" | "lastIndexOf" | "startsWith" | "includes" => &PARAMS_SEARCH_AND_POS,
            "endsWith" => &PARAMS_SEARCH_AND_END_POS,
            "slice" => &PARAMS_SLICE,
            "substring" | "substr" => &PARAMS_SUBSTRING,
            "padStart" | "padEnd" => &PARAMS_PAD,
            // match/matchAll/replace/replaceAll/search/split take RegExp or complex overloads.
            _ => return None,
        }),
        IntrinsicKind::Number => Some(match method {
            "valueOf" | "toLocaleString" => &[],
            "toFixed" => &PARAMS_OPT_DIGITS,
            "toExponential" => &PARAMS_OPT_FRACTION_DIGITS,
            "toPrecision" => &PARAMS_OPT_PRECISION,
            "toString" => &PARAMS_OPT_RADIX,
            _ => return None,
        }),
        IntrinsicKind::Boolean => Some(match method {
            "valueOf" | "toLocaleString" | "toString" => &[],
            _ => return None,
        }),
        IntrinsicKind::Bigint => Some(match method {
            "valueOf" | "toLocaleString" => &[],
            "toString" => &PARAMS_OPT_RADIX,
            _ => return None,
        }),
        _ => None,
    }
}
