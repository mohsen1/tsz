use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::parse_number_literal;

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

/// Name-free callable structure used inside object shapes.
///
/// Authored parameter names remain syntax/display provenance. They cannot
/// participate in semantic interning because renaming a binder does not
/// change a callable type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShapeParameter {
    pub ty: TypeId,
    pub optional: bool,
    pub rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShapeSignature {
    pub untyped_javascript: bool,
    pub parameters: Vec<ShapeParameter>,
    pub return_type: TypeId,
}

impl From<&Signature> for ShapeSignature {
    fn from(signature: &Signature) -> Self {
        Self {
            untyped_javascript: signature.untyped_javascript,
            parameters: signature
                .parameters
                .iter()
                .map(|parameter| ShapeParameter {
                    ty: parameter.ty,
                    optional: parameter.optional,
                    rest: parameter.rest,
                })
                .collect(),
            return_type: signature.return_type,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub call_signatures: Vec<ShapeSignature>,
    pub construct_signatures: Vec<ShapeSignature>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeferredBinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitwiseAnd,
    BitwiseOr,
    UnsignedRightShift,
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
    FlowReference {
        file: FileId,
        expression: NodeId,
        declaration: DeclId,
        declared: TypeId,
    },
    LexicalThis {
        file: FileId,
        expression: NodeId,
    },
    Call {
        callee: TypeId,
        argument_count: usize,
    },
    GenericCall,
    Construct {
        callee: TypeId,
        type_arguments: Vec<TypeId>,
        argument_count: usize,
    },
    Property {
        object: TypeId,
        name: String,
    },
    ElementAccess {
        object: TypeId,
        index: TypeId,
        mode: ElementAccessMode,
    },
    Predicate {
        parameter: String,
        asserted: Option<TypeId>,
        asserts: bool,
        parameter_is_bound: bool,
    },
    Binary {
        operator: DeferredBinaryOperator,
        left: TypeId,
        right: TypeId,
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
    BigIntLiteral,
    NumericRecovery,
    Utf16StringLiteral,
    UniqueSymbol,
    GenericFunction,
    ObjectShape,
}

impl DeferredType {
    pub(crate) const fn is_query_local(&self) -> bool {
        matches!(
            self,
            Self::Value(_) | Self::Binary { .. } | Self::FlowReference { .. }
        )
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
    Function(Signature),
    ShapeFunction(ShapeSignature),
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
    Function(Option<DeclId>, SignatureOrderKey),
    ShapeFunction(SignatureOrderKey),
    Deferred(DeferredOrderKey),
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PropertyOrderKey {
    name: String,
    ty: TypeOrderKey,
    optional: bool,
    readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ParameterOrderKey {
    ty: TypeOrderKey,
    optional: bool,
    rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SignatureOrderKey {
    untyped_javascript: bool,
    parameters: Vec<ParameterOrderKey>,
    return_type: Box<TypeOrderKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DeferredOrderKey {
    Reference(DeclId, Vec<TypeOrderKey>),
    Value(DeclId),
    FlowReference(FileId, NodeId, DeclId, Box<TypeOrderKey>),
    LexicalThis(FileId, NodeId),
    Call(Box<TypeOrderKey>, usize),
    GenericCall,
    Construct(Box<TypeOrderKey>, Vec<TypeOrderKey>, usize),
    Property(Box<TypeOrderKey>, String),
    ElementAccess(Box<TypeOrderKey>, Box<TypeOrderKey>, u8),
    Predicate(String, Option<Box<TypeOrderKey>>, bool, bool),
    Binary(u8, Box<TypeOrderKey>, Box<TypeOrderKey>),
    Logical(u8, Box<TypeOrderKey>, Box<TypeOrderKey>),
    Unary(u8, Box<TypeOrderKey>),
    KeyOf(Box<TypeOrderKey>),
    Conditional(
        Box<TypeOrderKey>,
        Box<TypeOrderKey>,
        Box<TypeOrderKey>,
        Box<TypeOrderKey>,
    ),
    Mapped(
        Box<TypeOrderKey>,
        Option<Box<TypeOrderKey>>,
        Box<TypeOrderKey>,
        Option<bool>,
        Option<bool>,
    ),
    IndexedAccess(Box<TypeOrderKey>, Box<TypeOrderKey>),
    BigIntLiteral,
    NumericRecovery,
    Utf16StringLiteral,
    UniqueSymbol,
    GenericFunction,
    ObjectShape,
}

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
        signature: &ShapeSignature,
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
        fixed: &[ShapeParameter],
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
            TypeKind::Function(signature) => {
                pending.extend(signature.parameters.iter().map(|parameter| parameter.ty));
                pending.push(signature.return_type);
            }
            TypeKind::ShapeFunction(signature) => {
                Self::push_signature_children(signature, pending);
            }
            TypeKind::Deferred(deferred) => Self::push_deferred_children(deferred, pending),
            TypeKind::Error
            | TypeKind::Any
            | TypeKind::Unknown
            | TypeKind::Never
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::ObjectKeyword
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::ClassConstructor { .. } => {}
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

    fn push_signature_children(signature: &ShapeSignature, pending: &mut Vec<TypeId>) {
        pending.extend(signature.parameters.iter().map(|parameter| parameter.ty));
        pending.push(signature.return_type);
    }

    fn push_deferred_children(deferred: &DeferredType, pending: &mut Vec<TypeId>) {
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
                ..
            } => {
                pending.push(*callee);
                pending.extend(type_arguments.iter().copied());
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
                pending.extend(name_type.iter().copied());
                pending.push(*value);
            }
            DeferredType::IndexedAccess { object, index } => pending.extend([*object, *index]),
            DeferredType::FlowReference { declared, .. } => pending.push(*declared),
            DeferredType::Value(_)
            | DeferredType::LexicalThis { .. }
            | DeferredType::GenericCall
            | DeferredType::BigIntLiteral
            | DeferredType::NumericRecovery
            | DeferredType::Utf16StringLiteral
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
                .map(|property| PropertyOrderKey {
                    name: property.name.clone(),
                    ty: nested(property.ty),
                    optional: property.optional,
                    readonly: property.readonly,
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
                    .map(|signature| self.shape_signature_order_key(signature, depth + 1))
                    .collect(),
                construct_signatures: shape
                    .construct_signatures
                    .iter()
                    .map(|signature| self.shape_signature_order_key(signature, depth + 1))
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
            TypeKind::Function(signature) => TypeOrderKey::Function(
                signature.generic_declaration,
                self.signature_order_key(signature, depth + 1),
            ),
            TypeKind::ShapeFunction(signature) => {
                TypeOrderKey::ShapeFunction(self.shape_signature_order_key(signature, depth + 1))
            }
            TypeKind::Deferred(deferred) => {
                TypeOrderKey::Deferred(self.deferred_order_key(deferred, depth + 1))
            }
        }
    }

    fn signature_order_key(&self, signature: &Signature, depth: usize) -> SignatureOrderKey {
        self.signature_parts_order_key(
            signature.untyped_javascript,
            signature
                .parameters
                .iter()
                .map(|parameter| (parameter.ty, parameter.optional, parameter.rest)),
            signature.return_type,
            depth,
        )
    }

    fn shape_signature_order_key(
        &self,
        signature: &ShapeSignature,
        depth: usize,
    ) -> SignatureOrderKey {
        self.signature_parts_order_key(
            signature.untyped_javascript,
            signature
                .parameters
                .iter()
                .map(|parameter| (parameter.ty, parameter.optional, parameter.rest)),
            signature.return_type,
            depth,
        )
    }

    fn signature_parts_order_key(
        &self,
        untyped_javascript: bool,
        parameters: impl Iterator<Item = (TypeId, bool, bool)>,
        return_type: TypeId,
        depth: usize,
    ) -> SignatureOrderKey {
        SignatureOrderKey {
            untyped_javascript,
            parameters: parameters
                .map(|(ty, optional, rest)| ParameterOrderKey {
                    ty: self.stable_order_key(ty, depth),
                    optional,
                    rest,
                })
                .collect(),
            return_type: Box::new(self.stable_order_key(return_type, depth)),
        }
    }

    fn deferred_order_key(&self, deferred: &DeferredType, depth: usize) -> DeferredOrderKey {
        let nested = |id| self.stable_order_key(id, depth + 1);
        match deferred {
            DeferredType::Reference {
                declaration,
                arguments,
            } => DeferredOrderKey::Reference(
                *declaration,
                arguments.iter().map(|argument| nested(*argument)).collect(),
            ),
            DeferredType::Value(declaration) => DeferredOrderKey::Value(*declaration),
            DeferredType::FlowReference {
                file,
                expression,
                declaration,
                declared,
            } => DeferredOrderKey::FlowReference(
                *file,
                *expression,
                *declaration,
                Box::new(nested(*declared)),
            ),
            DeferredType::LexicalThis { file, expression } => {
                DeferredOrderKey::LexicalThis(*file, *expression)
            }
            DeferredType::Call {
                callee,
                argument_count,
            } => DeferredOrderKey::Call(Box::new(nested(*callee)), *argument_count),
            DeferredType::GenericCall => DeferredOrderKey::GenericCall,
            DeferredType::Construct {
                callee,
                type_arguments,
                argument_count,
            } => DeferredOrderKey::Construct(
                Box::new(nested(*callee)),
                type_arguments
                    .iter()
                    .map(|argument| nested(*argument))
                    .collect(),
                *argument_count,
            ),
            DeferredType::Property { object, name } => {
                DeferredOrderKey::Property(Box::new(nested(*object)), name.clone())
            }
            DeferredType::ElementAccess {
                object,
                index,
                mode,
            } => DeferredOrderKey::ElementAccess(
                Box::new(nested(*object)),
                Box::new(nested(*index)),
                match mode {
                    ElementAccessMode::Read => 0,
                    ElementAccessMode::Write => 1,
                    ElementAccessMode::EvolvingArrayWrite => 2,
                },
            ),
            DeferredType::Predicate {
                parameter,
                asserted,
                asserts,
                parameter_is_bound,
            } => DeferredOrderKey::Predicate(
                parameter.clone(),
                asserted.map(|asserted| Box::new(nested(asserted))),
                *asserts,
                *parameter_is_bound,
            ),
            DeferredType::Binary {
                operator,
                left,
                right,
            } => DeferredOrderKey::Binary(
                match operator {
                    DeferredBinaryOperator::Add => 0,
                    DeferredBinaryOperator::Subtract => 1,
                    DeferredBinaryOperator::Multiply => 2,
                    DeferredBinaryOperator::Divide => 3,
                    DeferredBinaryOperator::Remainder => 4,
                    DeferredBinaryOperator::BitwiseAnd => 5,
                    DeferredBinaryOperator::BitwiseOr => 6,
                    DeferredBinaryOperator::UnsignedRightShift => 7,
                },
                Box::new(nested(*left)),
                Box::new(nested(*right)),
            ),
            DeferredType::Logical {
                operator,
                left,
                right,
            } => DeferredOrderKey::Logical(
                match operator {
                    DeferredLogicalOperator::And => 0,
                    DeferredLogicalOperator::Or => 1,
                    DeferredLogicalOperator::Nullish => 2,
                },
                Box::new(nested(*left)),
                Box::new(nested(*right)),
            ),
            DeferredType::Unary { operator, operand } => DeferredOrderKey::Unary(
                match operator {
                    DeferredUnaryOperator::Plus => 0,
                    DeferredUnaryOperator::Minus => 1,
                    DeferredUnaryOperator::BitwiseNot => 2,
                    DeferredUnaryOperator::Await => 3,
                },
                Box::new(nested(*operand)),
            ),
            DeferredType::KeyOf(operand) => DeferredOrderKey::KeyOf(Box::new(nested(*operand))),
            DeferredType::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => DeferredOrderKey::Conditional(
                Box::new(nested(*check)),
                Box::new(nested(*extends)),
                Box::new(nested(*when_true)),
                Box::new(nested(*when_false)),
            ),
            DeferredType::Mapped {
                constraint,
                name_type,
                value,
                readonly,
                optional,
            } => DeferredOrderKey::Mapped(
                Box::new(nested(*constraint)),
                name_type.map(|name_type| Box::new(nested(name_type))),
                Box::new(nested(*value)),
                *readonly,
                *optional,
            ),
            DeferredType::IndexedAccess { object, index } => {
                DeferredOrderKey::IndexedAccess(Box::new(nested(*object)), Box::new(nested(*index)))
            }
            DeferredType::BigIntLiteral => DeferredOrderKey::BigIntLiteral,
            DeferredType::NumericRecovery => DeferredOrderKey::NumericRecovery,
            DeferredType::Utf16StringLiteral => DeferredOrderKey::Utf16StringLiteral,
            DeferredType::UniqueSymbol => DeferredOrderKey::UniqueSymbol,
            DeferredType::GenericFunction => DeferredOrderKey::GenericFunction,
            DeferredType::ObjectShape => DeferredOrderKey::ObjectShape,
        }
    }

    /// Return TypeScript's widened literal type while preserving regular
    /// literals from explicit annotations. This is the sole owner of literal
    /// provenance interpretation; checker call sites only choose when a
    /// mutable observation requires widening.
    pub fn widened_literal_type(&mut self, id: TypeId) -> TypeId {
        match self.kind(id).clone() {
            TypeKind::LiteralString(_, LiteralProvenance::Fresh) => self.builtins.string,
            TypeKind::LiteralNumber(_, LiteralProvenance::Fresh) => self.builtins.number,
            TypeKind::LiteralBoolean(_, LiteralProvenance::Fresh) => self.builtins.boolean,
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
            TypeKind::LiteralString(value, _) => format!("\"{value}\""),
            TypeKind::TypeParameter { name, .. } => name.clone(),
            TypeKind::ClassInstance {
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
                    DeferredBinaryOperator::BitwiseOr => "|",
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
            TypeKind::Deferred(DeferredType::UniqueSymbol) => "unique symbol".to_string(),
            TypeKind::Deferred(DeferredType::GenericFunction) => {
                "deferred-generic-function".to_string()
            }
            TypeKind::Deferred(DeferredType::ObjectShape) => "deferred-object".to_string(),
        }
    }

    fn display_shape_signature(
        &self,
        signature: &ShapeSignature,
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
