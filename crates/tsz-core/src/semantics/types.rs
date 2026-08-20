use std::collections::HashMap;

use crate::source::DeclId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Property {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParameterType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
    pub rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    pub parameters: Vec<ParameterType>,
    pub return_type: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeferredLogicalOperator {
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeferredUnaryOperator {
    Plus,
    Minus,
    BitwiseNot,
    Await,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeferredType {
    Reference {
        declaration: DeclId,
        arguments: Vec<TypeId>,
    },
    Value(DeclId),
    Call(TypeId),
    Property {
        object: TypeId,
        name: String,
    },
    Logical {
        operator: DeferredLogicalOperator,
        left: TypeId,
        right: TypeId,
    },
    Unary {
        operator: DeferredUnaryOperator,
        operand: TypeId,
    },
    KeyOf(TypeId),
    Conditional {
        check: TypeId,
        extends: TypeId,
        when_true: TypeId,
        when_false: TypeId,
    },
    Mapped {
        constraint: TypeId,
        name_type: Option<TypeId>,
        value: TypeId,
        readonly: Option<bool>,
        optional: Option<bool>,
    },
    IndexedAccess {
        object: TypeId,
        index: TypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Error,
    Any,
    Unknown,
    Never,
    Void,
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    BigInt,
    ObjectKeyword,
    Symbol,
    LiteralBoolean(bool),
    LiteralNumber(String),
    LiteralString(String),
    TypeParameter {
        declaration: DeclId,
        index: u32,
        name: String,
    },
    Array(TypeId),
    Tuple(Vec<TypeId>),
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    Object(Vec<Property>),
    Function(Signature),
    Deferred(DeferredType),
}

#[derive(Debug, Clone, Copy)]
pub struct BuiltinTypes {
    pub error: TypeId,
    pub any: TypeId,
    pub unknown: TypeId,
    pub never: TypeId,
    pub void: TypeId,
    pub undefined: TypeId,
    pub null: TypeId,
    pub boolean: TypeId,
    pub number: TypeId,
    pub string: TypeId,
    pub bigint: TypeId,
    pub object: TypeId,
    pub symbol: TypeId,
}

#[derive(Debug)]
pub struct TypeStore {
    kinds: Vec<TypeKind>,
    interned: HashMap<TypeKind, TypeId>,
    pub builtins: BuiltinTypes,
}

impl Default for TypeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeStore {
    #[must_use]
    pub fn new() -> Self {
        let mut store = Self {
            kinds: Vec::new(),
            interned: HashMap::new(),
            builtins: BuiltinTypes {
                error: TypeId(0),
                any: TypeId(0),
                unknown: TypeId(0),
                never: TypeId(0),
                void: TypeId(0),
                undefined: TypeId(0),
                null: TypeId(0),
                boolean: TypeId(0),
                number: TypeId(0),
                string: TypeId(0),
                bigint: TypeId(0),
                object: TypeId(0),
                symbol: TypeId(0),
            },
        };
        store.builtins = BuiltinTypes {
            error: store.intern(TypeKind::Error),
            any: store.intern(TypeKind::Any),
            unknown: store.intern(TypeKind::Unknown),
            never: store.intern(TypeKind::Never),
            void: store.intern(TypeKind::Void),
            undefined: store.intern(TypeKind::Undefined),
            null: store.intern(TypeKind::Null),
            boolean: store.intern(TypeKind::Boolean),
            number: store.intern(TypeKind::Number),
            string: store.intern(TypeKind::String),
            bigint: store.intern(TypeKind::BigInt),
            object: store.intern(TypeKind::ObjectKeyword),
            symbol: store.intern(TypeKind::Symbol),
        };
        store
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.kinds.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    #[must_use]
    pub fn kind(&self, id: TypeId) -> &TypeKind {
        &self.kinds[id.0 as usize]
    }

    pub fn intern(&mut self, kind: TypeKind) -> TypeId {
        if let Some(id) = self.interned.get(&kind) {
            return *id;
        }
        let id = TypeId(self.kinds.len() as u32);
        self.kinds.push(kind.clone());
        self.interned.insert(kind, id);
        id
    }

    pub fn union(&mut self, members: impl IntoIterator<Item = TypeId>) -> TypeId {
        let mut members: Vec<TypeId> = members.into_iter().collect();
        members.sort_unstable();
        members.dedup();
        match members.as_slice() {
            [] => self.builtins.never,
            [only] => *only,
            _ => self.intern(TypeKind::Union(members)),
        }
    }

    pub fn intersection(&mut self, members: impl IntoIterator<Item = TypeId>) -> TypeId {
        let mut members: Vec<TypeId> = members.into_iter().collect();
        members.sort_unstable();
        members.dedup();
        match members.as_slice() {
            [] => self.builtins.unknown,
            [only] => *only,
            _ => self.intern(TypeKind::Intersection(members)),
        }
    }

    pub fn object(&mut self, mut properties: Vec<Property>) -> TypeId {
        properties.sort_by(|left, right| left.name.cmp(&right.name));
        self.intern(TypeKind::Object(properties))
    }

    pub fn display(&self, id: TypeId) -> String {
        self.display_inner(id, 0)
    }

    fn display_inner(&self, id: TypeId, depth: usize) -> String {
        if depth > 24 {
            return "...".to_string();
        }
        match self.kind(id) {
            TypeKind::Error => "error".to_string(),
            TypeKind::Any => "any".to_string(),
            TypeKind::Unknown => "unknown".to_string(),
            TypeKind::Never => "never".to_string(),
            TypeKind::Void => "void".to_string(),
            TypeKind::Undefined => "undefined".to_string(),
            TypeKind::Null => "null".to_string(),
            TypeKind::Boolean => "boolean".to_string(),
            TypeKind::Number => "number".to_string(),
            TypeKind::String => "string".to_string(),
            TypeKind::BigInt => "bigint".to_string(),
            TypeKind::ObjectKeyword => "object".to_string(),
            TypeKind::Symbol => "symbol".to_string(),
            TypeKind::LiteralBoolean(value) => value.to_string(),
            TypeKind::LiteralNumber(value) => value.clone(),
            TypeKind::LiteralString(value) => format!("\"{value}\""),
            TypeKind::TypeParameter { name, .. } => name.clone(),
            TypeKind::Array(element) => format!("{}[]", self.display_inner(*element, depth + 1)),
            TypeKind::Tuple(elements) => format!(
                "[{}]",
                elements
                    .iter()
                    .map(|element| self.display_inner(*element, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypeKind::Union(members) => members
                .iter()
                .map(|member| self.display_inner(*member, depth + 1))
                .collect::<Vec<_>>()
                .join(" | "),
            TypeKind::Intersection(members) => members
                .iter()
                .map(|member| self.display_inner(*member, depth + 1))
                .collect::<Vec<_>>()
                .join(" & "),
            TypeKind::Object(properties) => format!(
                "{{ {} }}",
                properties
                    .iter()
                    .map(|property| format!(
                        "{}{}: {}",
                        property.name,
                        if property.optional { "?" } else { "" },
                        self.display_inner(property.ty, depth + 1)
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            TypeKind::Function(signature) => format!(
                "({}) => {}",
                signature
                    .parameters
                    .iter()
                    .map(|parameter| format!(
                        "{}{}: {}",
                        if parameter.rest {
                            format!("...{}", parameter.name)
                        } else {
                            parameter.name.clone()
                        },
                        if parameter.optional { "?" } else { "" },
                        self.display_inner(parameter.ty, depth + 1)
                    ))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.display_inner(signature.return_type, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::Reference { declaration, .. }) => {
                format!("deferred#{}:{}", declaration.file.0, declaration.local)
            }
            TypeKind::Deferred(DeferredType::Value(declaration)) => {
                format!("value#{}:{}", declaration.file.0, declaration.local)
            }
            TypeKind::Deferred(DeferredType::Call(callee)) => {
                format!("call {}", self.display_inner(*callee, depth + 1))
            }
            TypeKind::Deferred(DeferredType::Property { object, name }) => {
                format!("{}.{}", self.display_inner(*object, depth + 1), name)
            }
            TypeKind::Deferred(DeferredType::Logical {
                operator,
                left,
                right,
            }) => format!(
                "{} {} {}",
                self.display_inner(*left, depth + 1),
                match operator {
                    DeferredLogicalOperator::And => "&&",
                    DeferredLogicalOperator::Or => "||",
                    DeferredLogicalOperator::Nullish => "??",
                },
                self.display_inner(*right, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::Unary { operator, operand }) => format!(
                "{}{}",
                match operator {
                    DeferredUnaryOperator::Plus => "+",
                    DeferredUnaryOperator::Minus => "-",
                    DeferredUnaryOperator::BitwiseNot => "~",
                    DeferredUnaryOperator::Await => "await ",
                },
                self.display_inner(*operand, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::KeyOf(operand)) => {
                format!("keyof {}", self.display_inner(*operand, depth + 1))
            }
            TypeKind::Deferred(DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            }) => format!(
                "{} extends {} ? {} : {}",
                self.display_inner(*check, depth + 1),
                self.display_inner(*extends, depth + 1),
                self.display_inner(*when_true, depth + 1),
                self.display_inner(*when_false, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::Mapped {
                constraint,
                name_type,
                value,
                readonly,
                optional,
            }) => format!(
                "{{ {}[K in {}{}]{}: {} }}",
                match readonly {
                    Some(true) => "readonly ",
                    Some(false) => "-readonly ",
                    None => "",
                },
                self.display_inner(*constraint, depth + 1),
                name_type.map_or_else(String::new, |name_type| format!(
                    " as {}",
                    self.display_inner(name_type, depth + 1)
                )),
                match optional {
                    Some(true) => "?",
                    Some(false) => "-?",
                    None => "",
                },
                self.display_inner(*value, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::IndexedAccess { object, index }) => format!(
                "{}[{}]",
                self.display_inner(*object, depth + 1),
                self.display_inner(*index, depth + 1)
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion<T> {
    Complete(T),
    Deferred,
    Cycle,
    Limit,
}

impl<T> Completion<T> {
    #[must_use]
    pub fn map<U>(self, operation: impl FnOnce(T) -> U) -> Completion<U> {
        match self {
            Self::Complete(value) => Completion::Complete(operation(value)),
            Self::Deferred => Completion::Deferred,
            Self::Cycle => Completion::Cycle,
            Self::Limit => Completion::Limit,
        }
    }
}
