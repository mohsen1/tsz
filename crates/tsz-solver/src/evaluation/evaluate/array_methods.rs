//! Array method return categories used by apparent type computation.

/// Array methods that return any.
pub(crate) const ARRAY_METHODS_RETURN_ANY: &[&str] = &[
    "concat",
    "filter",
    "flat",
    "flatMap",
    "map",
    "reverse",
    "slice",
    "sort",
    "splice",
    "toReversed",
    "toSorted",
    "toSpliced",
    "with",
    "at",
    "find",
    "findLast",
    "pop",
    "shift",
    "entries",
    "keys",
    "values",
    "reduce",
    "reduceRight",
];

/// Array methods that return boolean.
pub(crate) const ARRAY_METHODS_RETURN_BOOLEAN: &[&str] = &["every", "includes", "some"];

/// Array methods that return number.
pub(crate) const ARRAY_METHODS_RETURN_NUMBER: &[&str] = &[
    "findIndex",
    "findLastIndex",
    "indexOf",
    "lastIndexOf",
    "push",
    "unshift",
];

/// Array methods that return void.
pub(crate) const ARRAY_METHODS_RETURN_VOID: &[&str] = &["forEach", "copyWithin", "fill"];

/// Array methods that return string.
pub(crate) const ARRAY_METHODS_RETURN_STRING: &[&str] = &["join", "toLocaleString", "toString"];
