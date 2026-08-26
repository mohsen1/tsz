use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{KeywordType, parse_number_literal};
use crate::text::quote_string;

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
    pub name: Option<String>,
    pub ty: TypeId,
    pub optional: bool,
    pub rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    /// Declaration that owns this signature's authored type parameters.
    ///
    /// This survives value aliases so call inference can distinguish the
    /// signature's own uninstantiated binders from type parameters captured
    /// from an enclosing declaration.
    pub generic_declaration: Option<DeclId>,
    pub untyped_javascript: bool,
    pub parameters: Vec<ParameterType>,
    pub return_type: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallArityGap {
    Reference(TypeId, DeclId, Vec<TypeId>),
    Type(TypeId),
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallArityResolution {
    Expanded(TypeId),
    OpaqueRequired,
    RestArray(TypeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallOmission {
    Required,
    Omittable,
    Absorbing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexKeyKind {
    String,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ElementAccessMode {
    Read,
    Write,
    EvolvingArrayWrite,
}

impl ElementAccessMode {
    pub(crate) const fn is_read(self) -> bool {
        matches!(self, Self::Read)
    }

    pub(crate) const fn is_write(self) -> bool {
        matches!(self, Self::Write | Self::EvolvingArrayWrite)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IndexSignature {
    pub key: IndexKeyKind,
    pub value: TypeId,
    pub readonly: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ObjectShape {
    pub properties: Vec<Property>,
    pub call_signatures: Vec<Signature>,
    pub construct_signatures: Vec<Signature>,
    pub index_signatures: Vec<IndexSignature>,
}

impl ObjectShape {
    #[must_use]
    pub fn index(&self, key: IndexKeyKind) -> Option<&IndexSignature> {
        self.index_signatures.iter().find(|index| index.key == key)
    }
}

impl From<Vec<Property>> for ObjectShape {
    fn from(properties: Vec<Property>) -> Self {
        Self {
            properties,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DeferredBinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    LeftShift,
    SignedRightShift,
    UnsignedRightShift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DeferredLogicalOperator {
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DeferredUnaryOperator {
    Plus,
    Minus,
    BitwiseNot,
    Await,
    NonNull,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeferredType<T = TypeId> {
    Reference {
        declaration: DeclId,
        arguments: Vec<T>,
    },
    Value(DeclId),
    FlowReference {
        file: FileId,
        expression: NodeId,
        declaration: DeclId,
        declared: T,
    },
    LexicalThis {
        file: FileId,
        expression: NodeId,
    },
    Call {
        callee: T,
        argument_count: usize,
    },
    GenericCall,
    Construct {
        callee: T,
        type_arguments: Vec<T>,
        arguments: Vec<T>,
    },
    Property {
        object: T,
        name: String,
    },
    ElementAccess {
        object: T,
        index: T,
        mode: ElementAccessMode,
    },
    Predicate {
        parameter: String,
        asserted: Option<T>,
        asserts: bool,
        parameter_is_bound: bool,
    },
    Binary {
        operator: DeferredBinaryOperator,
        left: T,
        right: T,
    },
    Logical {
        operator: DeferredLogicalOperator,
        left: T,
        right: T,
    },
    Unary {
        operator: DeferredUnaryOperator,
        operand: T,
    },
    KeyOf(T),
    Conditional {
        check: T,
        extends: T,
        when_true: T,
        when_false: T,
    },
    Mapped {
        constraint: T,
        name_type: Option<T>,
        value: T,
        readonly: Option<bool>,
        optional: Option<bool>,
    },
    IndexedAccess {
        object: T,
        index: T,
    },
    BigIntLiteral,
    NumericRecovery,
    Utf16StringLiteral,
    TemplateValue,
    UniqueSymbol,
    GenericFunction,
    ObjectShape,
}

impl<T> DeferredType<T> {
    pub(crate) const fn is_query_local(&self) -> bool {
        matches!(
            self,
            Self::Value(_) | Self::Binary { .. } | Self::FlowReference { .. }
        )
    }

    fn map_types<U>(self, mut map: impl FnMut(T) -> U) -> DeferredType<U> {
        match self {
            Self::Reference {
                declaration,
                arguments,
            } => DeferredType::Reference {
                declaration,
                arguments: arguments.into_iter().map(&mut map).collect(),
            },
            Self::Value(declaration) => DeferredType::Value(declaration),
            Self::FlowReference {
                file,
                expression,
                declaration,
                declared,
            } => DeferredType::FlowReference {
                file,
                expression,
                declaration,
                declared: map(declared),
            },
            Self::LexicalThis { file, expression } => {
                DeferredType::LexicalThis { file, expression }
            }
            Self::Call {
                callee,
                argument_count,
            } => DeferredType::Call {
                callee: map(callee),
                argument_count,
            },
            Self::GenericCall => DeferredType::GenericCall,
            Self::Construct {
                callee,
                type_arguments,
                arguments,
            } => DeferredType::Construct {
                callee: map(callee),
                type_arguments: type_arguments.into_iter().map(&mut map).collect(),
                arguments: arguments.into_iter().map(&mut map).collect(),
            },
            Self::Property { object, name } => DeferredType::Property {
                object: map(object),
                name,
            },
            Self::ElementAccess {
                object,
                index,
                mode,
            } => DeferredType::ElementAccess {
                object: map(object),
                index: map(index),
                mode,
            },
            Self::Predicate {
                parameter,
                asserted,
                asserts,
                parameter_is_bound,
            } => DeferredType::Predicate {
                parameter,
                asserted: asserted.map(map),
                asserts,
                parameter_is_bound,
            },
            Self::Binary {
                operator,
                left,
                right,
            } => DeferredType::Binary {
                operator,
                left: map(left),
                right: map(right),
            },
            Self::Logical {
                operator,
                left,
                right,
            } => DeferredType::Logical {
                operator,
                left: map(left),
                right: map(right),
            },
            Self::Unary { operator, operand } => DeferredType::Unary {
                operator,
                operand: map(operand),
            },
            Self::KeyOf(operand) => DeferredType::KeyOf(map(operand)),
            Self::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => DeferredType::Conditional {
                check: map(check),
                extends: map(extends),
                when_true: map(when_true),
                when_false: map(when_false),
            },
            Self::Mapped {
                constraint,
                name_type,
                value,
                readonly,
                optional,
            } => DeferredType::Mapped {
                constraint: map(constraint),
                name_type: name_type.map(&mut map),
                value: map(value),
                readonly,
                optional,
            },
            Self::IndexedAccess { object, index } => DeferredType::IndexedAccess {
                object: map(object),
                index: map(index),
            },
            Self::BigIntLiteral => DeferredType::BigIntLiteral,
            Self::NumericRecovery => DeferredType::NumericRecovery,
            Self::Utf16StringLiteral => DeferredType::Utf16StringLiteral,
            Self::TemplateValue => DeferredType::TemplateValue,
            Self::UniqueSymbol => DeferredType::UniqueSymbol,
            Self::GenericFunction => DeferredType::GenericFunction,
            Self::ObjectShape => DeferredType::ObjectShape,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InvalidType {
    MissingProperty { object: TypeId, name: String },
    MissingProperties { object: TypeId, names: Vec<String> },
}

/// Whether a literal type came from inference or an explicit type position.
///
/// This is the bounded counterpart of TypeScript's fresh/regular literal
/// pairing. Mutable observations widen only fresh literals; an explicitly
/// annotated literal is already regular and must keep its literal identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiteralProvenance {
    Fresh,
    Regular,
}

/// Canonical JavaScript numeric-literal value.
///
/// Syntax keeps the authored token, but semantic identity follows the value
/// TypeScript observes after JavaScript number parsing. In particular,
/// `9007199254740993` and `9007199254740992` are the same literal type, and
/// exponent spellings display through one canonical representation.
#[derive(Debug, Clone)]
pub struct NumericLiteral {
    value: f64,
    display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericLiteralParseError;

impl NumericLiteral {
    fn from_source(source: &str) -> Result<Self, NumericLiteralParseError> {
        let parsed = parse_number_literal(source).ok_or(NumericLiteralParseError)?;
        Ok(Self {
            value: parsed.value,
            display: parsed.display,
        })
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn is_truthy(&self) -> bool {
        self.value != 0.0
    }

    pub fn array_index(&self) -> Option<usize> {
        (self.value.is_finite()
            && self.value >= 0.0
            && self.value.fract() == 0.0
            && self.value <= usize::MAX as f64)
            .then_some(self.value as usize)
    }
}

impl PartialEq for NumericLiteral {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
    }
}

impl Eq for NumericLiteral {}

impl Hash for NumericLiteral {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.to_bits().hash(state);
    }
}

impl PartialOrd for NumericLiteral {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumericLiteral {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.total_cmp(&other.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Error,
    Invalid(InvalidType),
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
    LiteralBoolean(bool, LiteralProvenance),
    LiteralNumber(NumericLiteral, LiteralProvenance),
    LiteralString(String, LiteralProvenance),
    TypeParameter {
        declaration: DeclId,
        index: u32,
        name: String,
    },
    Array(TypeId),
    Tuple(Vec<TypeId>),
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    Object(ObjectShape),
    ClassInstance {
        declaration: DeclId,
        name: String,
        arguments: Vec<TypeId>,
        properties: ObjectShape,
    },
    ClassConstructor {
        declaration: DeclId,
        name: String,
    },
    LibraryReference {
        declaration: DeclId,
        name: String,
        arguments: Vec<TypeId>,
    },
    Function(Signature),
    ShapeFunction(Signature),
    Deferred(DeferredType),
}

/// Allocation-independent ordering input for union and intersection members.
///
/// `TypeId` is session-local storage identity, so it cannot decide semantic
/// member order. This key follows typed structure and stable declaration/span
/// identities without consulting rendered type text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TypeOrderKey {
    Error,
    Invalid(Box<TypeOrderKey>, Vec<String>),
    Any,
    Unknown,
    String,
    Number,
    BigInt,
    Symbol,
    LiteralString(String, bool),
    LiteralNumber(NumericLiteral, bool),
    LiteralBoolean(bool, bool),
    Boolean,
    Null,
    Undefined,
    Void,
    Never,
    TypeParameter(DeclId, u32),
    ObjectKeyword,
    Array(Box<TypeOrderKey>),
    Tuple(Vec<TypeOrderKey>),
    Union(Vec<TypeOrderKey>),
    Intersection(Vec<TypeOrderKey>),
    Object {
        properties: Vec<PropertyOrderKey>,
        call_signatures: Vec<SignatureOrderKey>,
        construct_signatures: Vec<SignatureOrderKey>,
        index_signatures: Vec<(IndexKeyKind, Box<TypeOrderKey>, bool)>,
    },
    ClassInstance(DeclId, Vec<TypeOrderKey>, Vec<PropertyOrderKey>),
    ClassConstructor(DeclId),
    LibraryReference(DeclId, Vec<TypeOrderKey>),
    Function(Option<DeclId>, SignatureOrderKey),
    ShapeFunction(SignatureOrderKey),
    Deferred(DeferredOrderKey),
    Truncated,
}

type PropertyOrderKey = (String, TypeOrderKey, bool, bool);
type ParameterOrderKey = (TypeOrderKey, bool, bool);
type SignatureOrderKey = (bool, Vec<ParameterOrderKey>, Box<TypeOrderKey>);

type DeferredOrderKey = DeferredType<Box<TypeOrderKey>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnionPolicy {
    Canonical,
    PreserveAuthoredStructuralOrder,
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

impl BuiltinTypes {
    pub(crate) const fn keyword(self, keyword: KeywordType) -> Option<TypeId> {
        Some(match keyword {
            KeywordType::Any => self.any,
            KeywordType::Unknown => self.unknown,
            KeywordType::Never => self.never,
            KeywordType::Void => self.void,
            KeywordType::Undefined => self.undefined,
            KeywordType::Null => self.null,
            KeywordType::Boolean => self.boolean,
            KeywordType::Number => self.number,
            KeywordType::String => self.string,
            KeywordType::BigInt => self.bigint,
            KeywordType::Object => self.object,
            KeywordType::Symbol => self.symbol,
            KeywordType::UniqueSymbol => return None,
        })
    }
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
    pub fn kind(&self, id: TypeId) -> &TypeKind {
        &self.kinds[id.0 as usize]
    }

    pub(crate) fn effective_call_arity(
        &self,
        signature: &Signature,
        references: &HashMap<TypeId, Completion<CallArityResolution>>,
    ) -> Result<(usize, Option<usize>), CallArityGap> {
        let parameters = &signature.parameters;
        let rest = parameters.iter().position(|parameter| parameter.rest);
        let fixed = rest.unwrap_or(parameters.len());
        let syntactic_minimum = parameters[..fixed]
            .iter()
            .rposition(|parameter| !signature.untyped_javascript && !parameter.optional)
            .map_or(0, |index| index + 1);
        let Some(rest) = rest else {
            let minimum =
                self.call_minimum(&parameters[..fixed], &[], syntactic_minimum, references)?;
            return Ok((minimum, Some(parameters.len())));
        };
        if rest + 1 != parameters.len() || parameters[rest].optional {
            return Err(CallArityGap::Deferred);
        }
        let fixed_minimum =
            || self.call_minimum(&parameters[..rest], &[], syntactic_minimum, references);
        let rest_type = match self.call_arity_type(parameters[rest].ty, references)? {
            CallArityResolution::Expanded(rest_type) => rest_type,
            CallArityResolution::RestArray(_) => return Ok((fixed_minimum()?, None)),
            CallArityResolution::OpaqueRequired => return Err(CallArityGap::Deferred),
        };
        match self.kind(rest_type) {
            TypeKind::Any
            | TypeKind::Never
            | TypeKind::Error
            | TypeKind::Invalid(_)
            | TypeKind::Array(_) => Ok((fixed_minimum()?, None)),
            TypeKind::Tuple(elements) => {
                let base = if elements.is_empty() {
                    syntactic_minimum
                } else {
                    rest + elements.len()
                };
                let minimum = self.call_minimum(&parameters[..rest], elements, base, references)?;
                Ok((minimum, Some(rest + elements.len())))
            }
            _ => Err(CallArityGap::Deferred),
        }
    }

    fn call_arity_type(
        &self,
        mut ty: TypeId,
        references: &HashMap<TypeId, Completion<CallArityResolution>>,
    ) -> Result<CallArityResolution, CallArityGap> {
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(ty) {
                return Err(CallArityGap::Deferred);
            }
            match references.get(&ty) {
                Some(Completion::Complete(CallArityResolution::Expanded(resolved))) => {
                    ty = *resolved
                }
                Some(Completion::Complete(resolved)) => return Ok(*resolved),
                Some(_) => return Err(CallArityGap::Deferred),
                None => match self.kind(ty) {
                    TypeKind::Deferred(DeferredType::Reference {
                        declaration,
                        arguments,
                    }) => {
                        return Err(CallArityGap::Reference(ty, *declaration, arguments.clone()));
                    }
                    TypeKind::Deferred(_) => return Err(CallArityGap::Type(ty)),
                    _ => return Ok(CallArityResolution::Expanded(ty)),
                },
            }
        }
    }

    fn call_minimum(
        &self,
        fixed: &[ParameterType],
        tail: &[TypeId],
        base: usize,
        references: &HashMap<TypeId, Completion<CallArityResolution>>,
    ) -> Result<usize, CallArityGap> {
        for index in (0..base).rev() {
            let ty = if index < fixed.len() {
                fixed[index].ty
            } else {
                tail[index - fixed.len()]
            };
            if self.call_omission(ty, references, &mut HashSet::new())? != CallOmission::Omittable {
                return Ok(index + 1);
            }
        }
        Ok(0)
    }

    fn call_omission(
        &self,
        ty: TypeId,
        references: &HashMap<TypeId, Completion<CallArityResolution>>,
        seen: &mut HashSet<TypeId>,
    ) -> Result<CallOmission, CallArityGap> {
        let ty = match self.call_arity_type(ty, references)? {
            CallArityResolution::Expanded(ty) => ty,
            _ => return Ok(CallOmission::Required),
        };
        if !seen.insert(ty) {
            return Err(CallArityGap::Deferred);
        }
        let result = match self.kind(ty) {
            TypeKind::Void => Ok(CallOmission::Omittable),
            TypeKind::Any | TypeKind::Unknown | TypeKind::Error | TypeKind::Invalid(_) => {
                Ok(CallOmission::Absorbing)
            }
            TypeKind::Intersection(_) => Err(CallArityGap::Deferred),
            TypeKind::Union(members) => {
                let mut omission = CallOmission::Required;
                let mut pending = None;
                for member in members {
                    match self.call_omission(*member, references, seen) {
                        Ok(CallOmission::Absorbing) => {
                            omission = CallOmission::Absorbing;
                            break;
                        }
                        Ok(CallOmission::Omittable) => omission = CallOmission::Omittable,
                        Err(query)
                            if !matches!(&query, CallArityGap::Deferred) || pending.is_none() =>
                        {
                            pending = Some(query);
                        }
                        Ok(CallOmission::Required) | Err(_) => {}
                    }
                }
                match omission {
                    CallOmission::Absorbing => Ok(omission),
                    _ => pending.map_or(Ok(omission), Err),
                }
            }
            _ => Ok(CallOmission::Required),
        };
        seen.remove(&ty);
        result
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

    /// Preserve a declaration-owned symbolic identity until the consuming
    /// checker query genuinely needs to instantiate or inspect its shape.
    pub(crate) fn symbolic_reference(
        &mut self,
        declaration: DeclId,
        arguments: Vec<TypeId>,
    ) -> TypeId {
        self.intern(TypeKind::Deferred(DeferredType::Reference {
            declaration,
            arguments,
        }))
    }

    pub(crate) fn type_parameter(&mut self, declaration: DeclId, index: u32, name: &str) -> TypeId {
        self.intern(TypeKind::TypeParameter {
            declaration,
            index,
            name: name.to_string(),
        })
    }

    /// Query an unapplied symbolic reference without exposing solver storage
    /// representation to checker feature modules.
    pub(crate) fn is_unapplied_symbolic_reference(&self, ty: TypeId, declaration: DeclId) -> bool {
        matches!(
            self.kind(ty),
            TypeKind::Deferred(DeferredType::Reference {
                declaration: candidate,
                arguments,
            }) if *candidate == declaration && arguments.is_empty()
        )
    }

    /// The binder positions from `declaration` consumed by a type graph.
    ///
    /// Generic-call instantiation uses these declaration-owned identities;
    /// authored binder names and rendered types are not semantic inputs.
    pub(crate) fn type_parameters_from(&self, root: TypeId, declaration: DeclId) -> HashSet<u32> {
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        let mut parameters = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match self.kind(ty) {
                TypeKind::TypeParameter {
                    declaration: owner,
                    index,
                    ..
                } if *owner == declaration => {
                    parameters.insert(*index);
                }
                kind => Self::push_type_children(kind, &mut pending),
            }
        }
        parameters
    }

    pub(crate) fn push_type_children(kind: &TypeKind, pending: &mut Vec<TypeId>) {
        match kind {
            TypeKind::Invalid(InvalidType::MissingProperty { object, .. })
            | TypeKind::Invalid(InvalidType::MissingProperties { object, .. })
            | TypeKind::Array(object) => pending.push(*object),
            TypeKind::Tuple(elements)
            | TypeKind::Union(elements)
            | TypeKind::Intersection(elements) => pending.extend(elements.iter().copied()),
            TypeKind::Object(shape) => Self::push_shape_children(shape, pending),
            TypeKind::ClassInstance {
                arguments,
                properties,
                ..
            } => {
                pending.extend(arguments.iter().copied());
                Self::push_shape_children(properties, pending);
            }
            TypeKind::LibraryReference { arguments, .. } => {
                pending.extend(arguments.iter().copied());
            }
            TypeKind::Function(signature) | TypeKind::ShapeFunction(signature) => {
                Self::push_signature_children(signature, pending);
            }
            TypeKind::Deferred(deferred) => Self::push_deferred_children(deferred, pending),
            TypeKind::Error | non_recursive_type_kind!() => {}
        }
    }

    fn push_shape_children(shape: &ObjectShape, pending: &mut Vec<TypeId>) {
        pending.extend(shape.properties.iter().map(|property| property.ty));
        for signature in shape
            .call_signatures
            .iter()
            .chain(&shape.construct_signatures)
        {
            Self::push_signature_children(signature, pending);
        }
        pending.extend(shape.index_signatures.iter().map(|index| index.value));
    }

    fn push_signature_children(signature: &Signature, pending: &mut Vec<TypeId>) {
        pending.extend(signature.parameters.iter().map(|parameter| parameter.ty));
        pending.push(signature.return_type);
    }

    pub(crate) fn push_deferred_children(deferred: &DeferredType, pending: &mut Vec<TypeId>) {
        match deferred {
            DeferredType::Reference { arguments, .. } => pending.extend(arguments.iter().copied()),
            DeferredType::Call { callee, .. } | DeferredType::Property { object: callee, .. } => {
                pending.push(*callee);
            }
            DeferredType::ElementAccess { object, index, .. } => {
                pending.extend([*object, *index]);
            }
            DeferredType::Construct {
                callee,
                type_arguments,
                arguments,
            } => {
                pending.push(*callee);
                pending.extend(type_arguments.iter().copied());
                pending.extend(arguments.iter().copied());
            }
            DeferredType::Predicate { asserted, .. } => pending.extend(asserted.iter().copied()),
            DeferredType::Binary { left, right, .. }
            | DeferredType::Logical { left, right, .. } => {
                pending.extend([*left, *right]);
            }
            DeferredType::Unary { operand, .. } | DeferredType::KeyOf(operand) => {
                pending.push(*operand);
            }
            DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => pending.extend([*check, *extends, *when_true, *when_false]),
            DeferredType::Mapped {
                constraint,
                name_type,
                value,
                ..
            } => {
                pending.push(*constraint);
                pending.push(*value);
                pending.extend(name_type.iter().copied());
            }
            DeferredType::IndexedAccess { object, index } => pending.extend([*object, *index]),
            DeferredType::FlowReference { declared, .. } => pending.push(*declared),
            DeferredType::Value(_)
            | DeferredType::LexicalThis { .. }
            | DeferredType::GenericCall
            | DeferredType::BigIntLiteral
            | DeferredType::NumericRecovery
            | DeferredType::Utf16StringLiteral
            | DeferredType::TemplateValue
            | DeferredType::UniqueSymbol
            | DeferredType::GenericFunction
            | DeferredType::ObjectShape => {}
        }
    }

    /// Allocate an incomplete anonymous shape without giving it interned
    /// semantic identity. Required boundaries always force this to
    /// `Completion::Deferred`, and definitive caches reject it.
    pub fn deferred_object_shape(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::ObjectShape)
    }

    /// Allocate an identity-free nonclaim for generic function/constructor
    /// syntax until a binder-owned function-type declaration identity exists.
    pub fn deferred_generic_function(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::GenericFunction)
    }

    /// Allocate a query-local nonclaim for a generic call whose authored
    /// signature has not yet been instantiated. Keeping this fresh prevents
    /// unrelated call sites from sharing recursion identity or a force entry.
    pub fn deferred_generic_call(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::GenericCall)
    }

    pub(crate) fn deferred_unary(
        &mut self,
        operator: DeferredUnaryOperator,
        operand: TypeId,
    ) -> TypeId {
        if operator == DeferredUnaryOperator::NonNull
            && let Completion::Complete(non_nullable) = self.non_nullable(operand)
        {
            return non_nullable;
        }
        self.intern(TypeKind::Deferred(DeferredType::Unary {
            operator,
            operand,
        }))
    }

    pub fn function(
        &mut self,
        generic_declaration: Option<DeclId>,
        untyped_javascript: bool,
        parameters: Vec<ParameterType>,
        return_type: TypeId,
    ) -> TypeId {
        self.intern(TypeKind::Function(Signature {
            generic_declaration,
            untyped_javascript,
            parameters,
            return_type,
        }))
    }

    /// Allocate a source-free nonclaim for `unique symbol` until its
    /// declaration-owned nominal identity and host grammar are modeled.
    pub fn deferred_unique_symbol(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::UniqueSymbol)
    }

    /// Preserve `BigInt` literal syntax without collapsing distinct values to
    /// `bigint` before canonical arbitrary-precision identity is modeled.
    pub fn deferred_bigint_literal(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::BigIntLiteral)
    }

    /// Allocate a fresh nonclaim for scanner recovery whose numeric value is
    /// not owned. Authored malformed text never enters literal interning.
    pub fn deferred_numeric_recovery(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::NumericRecovery)
    }

    /// Allocate an identity-free typed nonclaim for an ordinary string value
    /// that cannot be represented by Rust `String` without losing UTF-16
    /// code units. The authored units never enter the type interner.
    pub fn deferred_utf16_string_literal(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::Utf16StringLiteral)
    }

    pub fn deferred_template_value(&mut self) -> TypeId {
        self.fresh_deferred(DeferredType::TemplateValue)
    }

    fn fresh_deferred(&mut self, deferred: DeferredType) -> TypeId {
        let id = TypeId(self.kinds.len() as u32);
        self.kinds.push(TypeKind::Deferred(deferred));
        id
    }

    pub fn numeric_literal(&mut self, source: &str, provenance: LiteralProvenance) -> TypeId {
        self.try_numeric_literal(source, provenance)
            .unwrap_or(self.builtins.error)
    }

    pub fn negated_numeric_literal(
        &mut self,
        mut literal: NumericLiteral,
        provenance: LiteralProvenance,
    ) -> TypeId {
        literal.value = -literal.value;
        literal.display = literal
            .display
            .strip_prefix('-')
            .map_or_else(|| format!("-{}", literal.display), str::to_string);
        self.intern(TypeKind::LiteralNumber(literal, provenance))
    }

    /// Parse a syntax-owned numeric token without manufacturing a numeric
    /// identity for malformed recovery text.
    pub fn try_numeric_literal(
        &mut self,
        source: &str,
        provenance: LiteralProvenance,
    ) -> Result<TypeId, NumericLiteralParseError> {
        Ok(self.intern(TypeKind::LiteralNumber(
            NumericLiteral::from_source(source)?,
            provenance,
        )))
    }

    pub fn union(
        &mut self,
        members: impl IntoIterator<Item = TypeId>,
        policy: UnionPolicy,
    ) -> TypeId {
        let mut flattened = Vec::new();
        for member in members {
            match self.kind(member) {
                TypeKind::Union(nested) => flattened.extend(nested.iter().copied()),
                _ => flattened.push(member),
            }
        }

        if flattened
            .iter()
            .any(|member| matches!(self.kind(*member), TypeKind::Any))
        {
            return self.builtins.any;
        }
        if flattened
            .iter()
            .any(|member| matches!(self.kind(*member), TypeKind::Unknown))
        {
            return self.builtins.unknown;
        }
        if let Some(error) = flattened
            .iter()
            .find(|member| matches!(self.kind(**member), TypeKind::Error | TypeKind::Invalid(_)))
        {
            return *error;
        }

        flattened.retain(|member| !matches!(self.kind(*member), TypeKind::Never));
        let absorbs_string = flattened
            .iter()
            .any(|member| matches!(self.kind(*member), TypeKind::String));
        let absorbs_number = flattened
            .iter()
            .any(|member| matches!(self.kind(*member), TypeKind::Number));
        let absorbs_boolean = flattened
            .iter()
            .any(|member| matches!(self.kind(*member), TypeKind::Boolean));
        flattened.retain(|member| {
            !matches!(
                self.kind(*member),
                TypeKind::LiteralString(_, _) if absorbs_string
            ) && !matches!(
                self.kind(*member),
                TypeKind::LiteralNumber(_, _) if absorbs_number
            ) && !matches!(
                self.kind(*member),
                TypeKind::LiteralBoolean(_, _) if absorbs_boolean
            )
        });

        if !absorbs_boolean {
            let has_true = flattened
                .iter()
                .any(|member| matches!(self.kind(*member), TypeKind::LiteralBoolean(true, _)));
            let has_false = flattened
                .iter()
                .any(|member| matches!(self.kind(*member), TypeKind::LiteralBoolean(false, _)));
            if has_true && has_false {
                flattened
                    .retain(|member| !matches!(self.kind(*member), TypeKind::LiteralBoolean(_, _)));
                flattened.push(self.builtins.boolean);
            }
        }

        if policy == UnionPolicy::Canonical {
            flattened.sort_by_cached_key(|member| self.stable_order_key(*member, 0));
        }
        let mut seen = HashSet::new();
        flattened.retain(|member| seen.insert(*member));
        let members = flattened;
        match members.as_slice() {
            [] => self.builtins.never,
            [only] => *only,
            _ => self.intern(TypeKind::Union(members)),
        }
    }

    pub fn intersection(&mut self, members: impl IntoIterator<Item = TypeId>) -> TypeId {
        let mut members: Vec<TypeId> = members.into_iter().collect();
        members.sort_by_cached_key(|member| self.stable_order_key(*member, 0));
        members.dedup();
        match members.as_slice() {
            [] => self.builtins.unknown,
            [only] => *only,
            _ => self.intern(TypeKind::Intersection(members)),
        }
    }

    pub(crate) fn non_nullable(&mut self, ty: TypeId) -> Completion<TypeId> {
        use Completion::{Complete, Deferred};

        let nullish =
            |kind: &TypeKind| matches!(kind, TypeKind::Void | TypeKind::Null | TypeKind::Undefined);
        match self.kind(ty).clone() {
            TypeKind::Deferred(_) | TypeKind::TypeParameter { .. } => Deferred,
            TypeKind::Void | TypeKind::Null | TypeKind::Undefined => Complete(self.builtins.never),
            TypeKind::Unknown => Complete(self.object(Vec::new())),
            TypeKind::Union(mut types) => {
                if types
                    .iter()
                    .any(|&id| matches!(self.kind(id), TypeKind::TypeParameter { .. }))
                {
                    return Deferred;
                }
                types.retain(|&id| !nullish(self.kind(id)));
                Complete(self.union(types, UnionPolicy::Canonical))
            }
            _ => Complete(ty),
        }
    }

    pub fn object(&mut self, mut properties: Vec<Property>) -> TypeId {
        properties.sort_by(|left, right| left.name.cmp(&right.name));
        self.object_shape(ObjectShape {
            properties,
            ..ObjectShape::default()
        })
    }

    pub fn object_shape(&mut self, mut shape: ObjectShape) -> TypeId {
        shape
            .properties
            .sort_by(|left, right| left.name.cmp(&right.name));
        shape.index_signatures.sort_by_key(|index| index.key);
        self.intern(TypeKind::Object(shape))
    }

    fn stable_order_key(&self, id: TypeId, depth: usize) -> TypeOrderKey {
        if depth > 64 {
            return TypeOrderKey::Truncated;
        }
        let nested = |id| self.stable_order_key(id, depth + 1);
        let properties = |properties: &[Property]| {
            properties
                .iter()
                .map(|property| {
                    (
                        property.name.clone(),
                        nested(property.ty),
                        property.optional,
                        property.readonly,
                    )
                })
                .collect()
        };
        match self.kind(id) {
            TypeKind::Error => TypeOrderKey::Error,
            TypeKind::Invalid(InvalidType::MissingProperty { object, name }) => {
                TypeOrderKey::Invalid(Box::new(nested(*object)), vec![name.clone()])
            }
            TypeKind::Invalid(InvalidType::MissingProperties { object, names }) => {
                TypeOrderKey::Invalid(Box::new(nested(*object)), names.clone())
            }
            TypeKind::Any => TypeOrderKey::Any,
            TypeKind::Unknown => TypeOrderKey::Unknown,
            TypeKind::Never => TypeOrderKey::Never,
            TypeKind::Void => TypeOrderKey::Void,
            TypeKind::Undefined => TypeOrderKey::Undefined,
            TypeKind::Null => TypeOrderKey::Null,
            TypeKind::Boolean => TypeOrderKey::Boolean,
            TypeKind::Number => TypeOrderKey::Number,
            TypeKind::String => TypeOrderKey::String,
            TypeKind::BigInt => TypeOrderKey::BigInt,
            TypeKind::ObjectKeyword => TypeOrderKey::ObjectKeyword,
            TypeKind::Symbol => TypeOrderKey::Symbol,
            TypeKind::LiteralBoolean(value, provenance) => {
                TypeOrderKey::LiteralBoolean(*value, *provenance == LiteralProvenance::Regular)
            }
            TypeKind::LiteralNumber(value, provenance) => TypeOrderKey::LiteralNumber(
                value.clone(),
                *provenance == LiteralProvenance::Regular,
            ),
            TypeKind::LiteralString(value, provenance) => TypeOrderKey::LiteralString(
                value.clone(),
                *provenance == LiteralProvenance::Regular,
            ),
            TypeKind::TypeParameter {
                declaration, index, ..
            } => TypeOrderKey::TypeParameter(*declaration, *index),
            TypeKind::Array(element) => TypeOrderKey::Array(Box::new(nested(*element))),
            TypeKind::Tuple(elements) => {
                TypeOrderKey::Tuple(elements.iter().map(|element| nested(*element)).collect())
            }
            TypeKind::Union(members) => {
                TypeOrderKey::Union(members.iter().map(|member| nested(*member)).collect())
            }
            TypeKind::Intersection(members) => {
                TypeOrderKey::Intersection(members.iter().map(|member| nested(*member)).collect())
            }
            TypeKind::Object(shape) => TypeOrderKey::Object {
                properties: properties(&shape.properties),
                call_signatures: shape
                    .call_signatures
                    .iter()
                    .map(|signature| self.signature_order_key(signature, depth + 1))
                    .collect(),
                construct_signatures: shape
                    .construct_signatures
                    .iter()
                    .map(|signature| self.signature_order_key(signature, depth + 1))
                    .collect(),
                index_signatures: shape
                    .index_signatures
                    .iter()
                    .map(|index| (index.key, Box::new(nested(index.value)), index.readonly))
                    .collect(),
            },
            TypeKind::ClassInstance {
                declaration,
                arguments,
                properties: class_properties,
                ..
            } => TypeOrderKey::ClassInstance(
                *declaration,
                arguments.iter().map(|argument| nested(*argument)).collect(),
                properties(&class_properties.properties),
            ),
            TypeKind::ClassConstructor { declaration, .. } => {
                TypeOrderKey::ClassConstructor(*declaration)
            }
            TypeKind::LibraryReference {
                declaration,
                arguments,
                ..
            } => TypeOrderKey::LibraryReference(
                *declaration,
                arguments.iter().map(|argument| nested(*argument)).collect(),
            ),
            TypeKind::Function(signature) => TypeOrderKey::Function(
                signature.generic_declaration,
                self.signature_order_key(signature, depth + 1),
            ),
            TypeKind::ShapeFunction(signature) => {
                TypeOrderKey::ShapeFunction(self.signature_order_key(signature, depth + 1))
            }
            TypeKind::Deferred(deferred) => {
                TypeOrderKey::Deferred(self.deferred_order_key(deferred, depth + 1))
            }
        }
    }

    fn signature_order_key(&self, signature: &Signature, depth: usize) -> SignatureOrderKey {
        (
            signature.untyped_javascript,
            signature
                .parameters
                .iter()
                .map(|parameter| {
                    (
                        self.stable_order_key(parameter.ty, depth),
                        parameter.optional,
                        parameter.rest,
                    )
                })
                .collect(),
            Box::new(self.stable_order_key(signature.return_type, depth)),
        )
    }

    fn deferred_order_key(&self, deferred: &DeferredType, depth: usize) -> DeferredOrderKey {
        deferred
            .clone()
            .map_types(|id| Box::new(self.stable_order_key(id, depth + 1)))
    }

    /// Return TypeScript's widened literal type while preserving regular
    /// literals from explicit annotations. This is the sole owner of literal
    /// provenance interpretation; checker call sites only choose when a
    /// mutable observation requires widening.
    pub fn widened_literal_type(&mut self, id: TypeId) -> TypeId {
        match self.kind(id).clone() {
            TypeKind::LiteralString(_, LiteralProvenance::Fresh)
            | TypeKind::Deferred(DeferredType::Utf16StringLiteral) => self.builtins.string,
            TypeKind::LiteralNumber(_, LiteralProvenance::Fresh) => self.builtins.number,
            TypeKind::LiteralBoolean(_, LiteralProvenance::Fresh) => self.builtins.boolean,
            TypeKind::Deferred(DeferredType::BigIntLiteral) => self.builtins.bigint,
            TypeKind::Array(element) => {
                let widened = self.widened_literal_type(element);
                if widened == element {
                    id
                } else {
                    self.intern(TypeKind::Array(widened))
                }
            }
            TypeKind::Union(members) => {
                let widened = members
                    .iter()
                    .map(|member| self.widened_literal_type(*member))
                    .collect::<Vec<_>>();
                if widened == members {
                    id
                } else {
                    self.union(widened, UnionPolicy::Canonical)
                }
            }
            _ => id,
        }
    }

    pub fn display(&self, id: TypeId) -> String {
        self.display_inner(id, 0)
    }

    fn display_inner(&self, id: TypeId, depth: usize) -> String {
        if depth > 24 {
            return "...".to_string();
        }
        match self.kind(id) {
            TypeKind::Error | TypeKind::Invalid(_) => "error".to_string(),
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
            TypeKind::LiteralBoolean(value, _) => value.to_string(),
            TypeKind::LiteralNumber(value, _) => value.display().to_string(),
            TypeKind::LiteralString(value, _) => quote_string(value),
            TypeKind::TypeParameter { name, .. } => name.clone(),
            TypeKind::ClassInstance {
                name, arguments, ..
            }
            | TypeKind::LibraryReference {
                name, arguments, ..
            } => {
                if arguments.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{name}<{}>",
                        arguments
                            .iter()
                            .map(|argument| self.display_inner(*argument, depth + 1))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            TypeKind::Array(element) => {
                let element_name = self.display_inner(*element, depth + 1);
                if matches!(
                    self.kind(*element),
                    TypeKind::Union(_) | TypeKind::Intersection(_) | TypeKind::Function(_)
                ) {
                    format!("({element_name})[]")
                } else {
                    format!("{element_name}[]")
                }
            }
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
            TypeKind::Object(shape) => {
                if shape.properties.is_empty()
                    && shape.call_signatures.is_empty()
                    && shape.construct_signatures.is_empty()
                    && shape.index_signatures.is_empty()
                {
                    "{}".to_owned()
                } else {
                    let mut members = shape
                        .properties
                        .iter()
                        .map(|property| {
                            format!(
                                "{}{}: {}",
                                property.name,
                                if property.optional { "?" } else { "" },
                                self.display_inner(property.ty, depth + 1)
                            )
                        })
                        .collect::<Vec<_>>();
                    members.extend(shape.call_signatures.iter().map(|signature| {
                        self.display_shape_signature(signature, depth + 1, "", ": ")
                    }));
                    members.extend(shape.construct_signatures.iter().map(|signature| {
                        self.display_shape_signature(signature, depth + 1, "new ", ": ")
                    }));
                    members.extend(shape.index_signatures.iter().map(|index| {
                        format!(
                            "[key: {}]: {}",
                            match index.key {
                                IndexKeyKind::String => "string",
                                IndexKeyKind::Number => "number",
                            },
                            self.display_inner(index.value, depth + 1)
                        )
                    }));
                    format!("{{ {}; }}", members.join("; "))
                }
            }
            TypeKind::ClassConstructor { name, .. } => format!("typeof {name}"),
            TypeKind::Function(signature) => format!(
                "({}) => {}",
                signature
                    .parameters
                    .iter()
                    .map(|parameter| {
                        let name = parameter
                            .name
                            .as_deref()
                            .expect("authored function parameters retain their names");
                        format!(
                            "{}{}: {}",
                            if parameter.rest {
                                format!("...{name}")
                            } else {
                                name.to_string()
                            },
                            if parameter.optional { "?" } else { "" },
                            self.display_inner(parameter.ty, depth + 1)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                self.display_inner(signature.return_type, depth + 1)
            ),
            TypeKind::ShapeFunction(signature) => {
                self.display_shape_signature(signature, depth + 1, "", " => ")
            }
            TypeKind::Deferred(DeferredType::Reference { declaration, .. }) => {
                format!("deferred#{}:{}", declaration.file.0, declaration.local)
            }
            TypeKind::Deferred(DeferredType::Value(declaration)) => {
                format!("value#{}:{}", declaration.file.0, declaration.local)
            }
            TypeKind::Deferred(DeferredType::FlowReference { declared, .. }) => {
                self.display_inner(*declared, depth + 1)
            }
            TypeKind::Deferred(DeferredType::LexicalThis { .. }) => "this".to_string(),
            TypeKind::Deferred(DeferredType::Call { callee, .. }) => {
                format!("call {}", self.display_inner(*callee, depth + 1))
            }
            TypeKind::Deferred(DeferredType::GenericCall) => "deferred-generic-call".to_string(),
            TypeKind::Deferred(DeferredType::Construct {
                callee,
                type_arguments,
                ..
            }) => {
                let arguments = if type_arguments.is_empty() {
                    String::new()
                } else {
                    format!(
                        "<{}>",
                        type_arguments
                            .iter()
                            .map(|argument| self.display_inner(*argument, depth + 1))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!("new {}{arguments}", self.display_inner(*callee, depth + 1))
            }
            TypeKind::Deferred(DeferredType::Property { object, name }) => {
                format!("{}.{}", self.display_inner(*object, depth + 1), name)
            }
            TypeKind::Deferred(DeferredType::ElementAccess { object, index, .. })
            | TypeKind::Deferred(DeferredType::IndexedAccess { object, index }) => format!(
                "{}[{}]",
                self.display_inner(*object, depth + 1),
                self.display_inner(*index, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::Predicate {
                parameter,
                asserted,
                asserts,
                ..
            }) => format!(
                "{}{}{}",
                if *asserts { "asserts " } else { "" },
                parameter,
                asserted.map_or_else(String::new, |asserted| format!(
                    " is {}",
                    self.display_inner(asserted, depth + 1)
                ))
            ),
            TypeKind::Deferred(DeferredType::Binary {
                operator,
                left,
                right,
            }) => format!(
                "{} {} {}",
                self.display_inner(*left, depth + 1),
                match operator {
                    DeferredBinaryOperator::Add => "+",
                    DeferredBinaryOperator::Subtract => "-",
                    DeferredBinaryOperator::Multiply => "*",
                    DeferredBinaryOperator::Divide => "/",
                    DeferredBinaryOperator::Remainder => "%",
                    DeferredBinaryOperator::BitwiseAnd => "&",
                    DeferredBinaryOperator::BitwiseXor => "^",
                    DeferredBinaryOperator::BitwiseOr => "|",
                    DeferredBinaryOperator::LeftShift => "<<",
                    DeferredBinaryOperator::SignedRightShift => ">>",
                    DeferredBinaryOperator::UnsignedRightShift => ">>>",
                },
                self.display_inner(*right, depth + 1)
            ),
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
                    DeferredUnaryOperator::NonNull => "NonNullable ",
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
            TypeKind::Deferred(DeferredType::BigIntLiteral) => "bigint-literal".to_string(),
            TypeKind::Deferred(DeferredType::NumericRecovery) => {
                "deferred-numeric-recovery".to_string()
            }
            TypeKind::Deferred(DeferredType::Utf16StringLiteral) => {
                "deferred-utf16-string-literal".to_string()
            }
            TypeKind::Deferred(DeferredType::TemplateValue) => {
                "deferred-template-expression".to_string()
            }
            TypeKind::Deferred(DeferredType::UniqueSymbol) => "unique symbol".to_string(),
            TypeKind::Deferred(DeferredType::GenericFunction) => {
                "deferred-generic-function".to_string()
            }
            TypeKind::Deferred(DeferredType::ObjectShape) => "deferred-object".to_string(),
        }
    }

    fn display_shape_signature(
        &self,
        signature: &Signature,
        depth: usize,
        prefix: &str,
        return_separator: &str,
    ) -> String {
        format!(
            "{prefix}({}){return_separator}{}",
            signature
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| format!(
                    "{}arg{index}{}: {}",
                    if parameter.rest { "..." } else { "" },
                    if parameter.optional { "?" } else { "" },
                    self.display_inner(parameter.ty, depth + 1)
                ))
                .collect::<Vec<_>>()
                .join(", "),
            self.display_inner(signature.return_type, depth + 1)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion<T> {
    Complete(T),
    Deferred,
    Cycle,
    Limit,
}

#[cfg(test)]
#[path = "../../rewrite-tests/types_unit.rs"]
mod tests;
