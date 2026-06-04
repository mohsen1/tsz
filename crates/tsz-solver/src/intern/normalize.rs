use super::{TypeInterner, TypeListBuffer};

use crate::types::{
    CallableShape, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId,
    ParamInfo, PropertyInfo, TemplateLiteralId, TemplateSpan, TypeData, TypeId, Visibility,
};

use crate::visitor::is_literal_type;

use rustc_hash::{FxHashMap, FxHashSet};

use smallvec::SmallVec;

use std::sync::Arc;

use tsz_common::interner::Atom;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LiteralDomain {
    String,
    Number,
    Boolean,
    Bigint,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum UnitValueKey {
    Null,
    Undefined,
    String(Atom),
    Number(u64),
    Boolean(bool),
    BigInt(Atom),
    Enum(crate::def::DefId, Box<UnitValueKey>),
}

/// Primitive kind for disjoint intersection checking.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum PrimitiveKind {
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
}

impl PrimitiveKind {
    const fn from_literal(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::String(_) => Self::String,
            LiteralValue::Number(_) => Self::Number,
            LiteralValue::Boolean(_) => Self::Boolean,
            LiteralValue::BigInt(_) => Self::BigInt,
        }
    }
}

#[derive(Clone, Debug)]
enum LiteralKind {
    Single(LiteralValue),
    Union(LiteralDomain, FxHashSet<LiteralValue>),
}

impl LiteralKind {
    const fn domain(&self) -> LiteralDomain {
        match self {
            Self::Single(lit) => literal_domain(lit),
            Self::Union(domain, _) => *domain,
        }
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        if self.domain() != other.domain() {
            return true;
        }
        match (self, other) {
            (Self::Single(s), Self::Single(o)) => s != o,
            (Self::Single(s), Self::Union(_, set)) => !set.contains(s),
            (Self::Union(_, set), Self::Single(o)) => !set.contains(o),
            (Self::Union(_, s_set), Self::Union(_, o_set)) => {
                !s_set.iter().any(|v| o_set.contains(v))
            }
        }
    }
}

const fn literal_domain(literal: &LiteralValue) -> LiteralDomain {
    match literal {
        LiteralValue::String(_) => LiteralDomain::String,
        LiteralValue::Number(_) => LiteralDomain::Number,
        LiteralValue::Boolean(_) => LiteralDomain::Boolean,
        LiteralValue::BigInt(_) => LiteralDomain::Bigint,
    }
}

include!("normalize_parts/part1.rs");
include!("normalize_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TemplateSpan;

    #[test]
    fn shallow_subtype_skips_literal_to_template_literal_matching() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("foo-x");
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("foo-")),
            TemplateSpan::Type(TypeId::STRING),
        ]);

        assert!(
            !interner.is_subtype_shallow(literal, template),
            "union normalization should not invoke full template-literal subtype matching"
        );
    }
}
