use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

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
    pub parameters: Vec<ShapeParameter>,
    pub return_type: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexKeyKind {
    String,
    Number,
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
    Predicate {
        parameter: String,
        asserted: Option<TypeId>,
        asserts: bool,
        parameter_is_bound: bool,
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
    UniqueSymbol,
    GenericFunction,
    ObjectShape,
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
        let compact = source.replace('_', "");
        let value = if let Some(digits) = compact
            .strip_prefix("0x")
            .or_else(|| compact.strip_prefix("0X"))
        {
            parse_power_of_two_integer(digits, 4)?
        } else if let Some(digits) = compact
            .strip_prefix("0b")
            .or_else(|| compact.strip_prefix("0B"))
        {
            parse_power_of_two_integer(digits, 1)?
        } else if let Some(digits) = compact
            .strip_prefix("0o")
            .or_else(|| compact.strip_prefix("0O"))
        {
            parse_power_of_two_integer(digits, 3)?
        } else if compact.len() > 1
            && compact.starts_with('0')
            && compact.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
        {
            parse_power_of_two_integer(&compact[1..], 3)?
        } else {
            if !is_decimal_literal(&compact) {
                return Err(NumericLiteralParseError);
            }
            compact
                .parse::<f64>()
                .map_err(|_| NumericLiteralParseError)?
        };
        let value = if value == 0.0 { 0.0 } else { value };
        let display = javascript_number_to_string(value);
        Ok(Self { value, display })
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn is_truthy(&self) -> bool {
        self.value != 0.0
    }
}

fn is_decimal_literal(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut position = 0;
    let mut mantissa_digits = 0;

    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
        mantissa_digits += 1;
    }
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
            mantissa_digits += 1;
        }
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
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
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
fn parse_power_of_two_integer(
    digits: &str,
    bits_per_digit: usize,
) -> Result<f64, NumericLiteralParseError> {
    if digits.is_empty() {
        return Err(NumericLiteralParseError);
    }
    let radix = 1_u8 << bits_per_digit;
    let mut first_nonzero = None;
    for (index, byte) in digits.bytes().enumerate() {
        let Some(value) = radix_digit(byte) else {
            return Err(NumericLiteralParseError);
        };
        if value >= radix {
            return Err(NumericLiteralParseError);
        }
        if value != 0 && first_nonzero.is_none() {
            first_nonzero = Some((index, value));
        }
    }
    let Some((first_index, first_value)) = first_nonzero else {
        return Ok(0.0);
    };

    let first_width = (u8::BITS - first_value.leading_zeros()) as usize;
    let trailing_digits = digits.len() - first_index - 1;
    let Some(bit_length) = trailing_digits
        .checked_mul(bits_per_digit)
        .and_then(|width| width.checked_add(first_width))
    else {
        return Ok(f64::INFINITY);
    };
    if bit_length > 1024 {
        return Ok(f64::INFINITY);
    }

    let mut leading = 0_u64;
    let mut consumed = 0_usize;
    let mut guard = false;
    let mut sticky = false;
    for (relative_index, byte) in digits.as_bytes()[first_index..].iter().enumerate() {
        let value = radix_digit(*byte).ok_or(NumericLiteralParseError)?;
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
        return Ok(leading as f64);
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
        return Ok(f64::INFINITY);
    }
    let fraction = leading & ((1_u64 << 52) - 1);
    Ok(f64::from_bits(
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
    Function(Vec<ParameterOrderKey>, Box<TypeOrderKey>),
    ShapeFunction(Vec<ParameterOrderKey>, Box<TypeOrderKey>),
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
    parameters: Vec<ParameterOrderKey>,
    return_type: Box<TypeOrderKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DeferredOrderKey {
    Reference(DeclId, Vec<TypeOrderKey>),
    Value(DeclId),
    Call(Box<TypeOrderKey>, usize),
    GenericCall,
    Construct(Box<TypeOrderKey>, Vec<TypeOrderKey>, usize),
    Property(Box<TypeOrderKey>, String),
    Predicate(String, Option<Box<TypeOrderKey>>, bool, bool),
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

    /// Allocate an incomplete anonymous shape without giving it interned
    /// semantic identity. Required boundaries always force this to
    /// `Completion::Deferred`, and definitive caches reject it.
    pub fn deferred_object_shape(&mut self) -> TypeId {
        let id = TypeId(self.kinds.len() as u32);
        self.kinds
            .push(TypeKind::Deferred(DeferredType::ObjectShape));
        id
    }

    /// Allocate an identity-free nonclaim for generic function/constructor
    /// syntax until a binder-owned function-type declaration identity exists.
    pub fn deferred_generic_function(&mut self) -> TypeId {
        let id = TypeId(self.kinds.len() as u32);
        self.kinds
            .push(TypeKind::Deferred(DeferredType::GenericFunction));
        id
    }

    /// Allocate a source-free nonclaim for `unique symbol` until its
    /// declaration-owned nominal identity and host grammar are modeled.
    pub fn deferred_unique_symbol(&mut self) -> TypeId {
        let id = TypeId(self.kinds.len() as u32);
        self.kinds
            .push(TypeKind::Deferred(DeferredType::UniqueSymbol));
        id
    }

    /// Preserve `BigInt` literal syntax without collapsing distinct values to
    /// `bigint` before canonical arbitrary-precision identity is modeled.
    pub fn deferred_bigint_literal(&mut self) -> TypeId {
        let id = TypeId(self.kinds.len() as u32);
        self.kinds
            .push(TypeKind::Deferred(DeferredType::BigIntLiteral));
        id
    }

    pub fn numeric_literal(&mut self, source: &str, provenance: LiteralProvenance) -> TypeId {
        self.try_numeric_literal(source, provenance)
            .unwrap_or(self.builtins.error)
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
                    .map(|signature| SignatureOrderKey {
                        parameters: signature
                            .parameters
                            .iter()
                            .map(|parameter| ParameterOrderKey {
                                ty: nested(parameter.ty),
                                optional: parameter.optional,
                                rest: parameter.rest,
                            })
                            .collect(),
                        return_type: Box::new(nested(signature.return_type)),
                    })
                    .collect(),
                construct_signatures: shape
                    .construct_signatures
                    .iter()
                    .map(|signature| SignatureOrderKey {
                        parameters: signature
                            .parameters
                            .iter()
                            .map(|parameter| ParameterOrderKey {
                                ty: nested(parameter.ty),
                                optional: parameter.optional,
                                rest: parameter.rest,
                            })
                            .collect(),
                        return_type: Box::new(nested(signature.return_type)),
                    })
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
                signature
                    .parameters
                    .iter()
                    .map(|parameter| ParameterOrderKey {
                        ty: nested(parameter.ty),
                        optional: parameter.optional,
                        rest: parameter.rest,
                    })
                    .collect(),
                Box::new(nested(signature.return_type)),
            ),
            TypeKind::ShapeFunction(signature) => TypeOrderKey::ShapeFunction(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| ParameterOrderKey {
                        ty: nested(parameter.ty),
                        optional: parameter.optional,
                        rest: parameter.rest,
                    })
                    .collect(),
                Box::new(nested(signature.return_type)),
            ),
            TypeKind::Deferred(deferred) => {
                TypeOrderKey::Deferred(self.deferred_order_key(deferred, depth + 1))
            }
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
            TypeKind::Deferred(DeferredType::IndexedAccess { object, index, .. }) => format!(
                "{}[{}]",
                self.display_inner(*object, depth + 1),
                self.display_inner(*index, depth + 1)
            ),
            TypeKind::Deferred(DeferredType::BigIntLiteral) => "bigint-literal".to_string(),
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
mod tests {
    use super::*;

    fn literal_array(store: &mut TypeStore, value: &str) -> TypeId {
        let literal = store.intern(TypeKind::LiteralString(
            value.to_string(),
            LiteralProvenance::Regular,
        ));
        store.intern(TypeKind::Array(literal))
    }

    #[test]
    fn union_order_follows_typed_structure_not_allocation_or_input_order() {
        let mut reverse_allocation = TypeStore::new();
        let reverse_b = literal_array(&mut reverse_allocation, "b");
        let reverse_a = literal_array(&mut reverse_allocation, "a");
        let reverse_union =
            reverse_allocation.union([reverse_b, reverse_a], UnionPolicy::Canonical);

        let mut forward_allocation = TypeStore::new();
        let forward_a = literal_array(&mut forward_allocation, "a");
        let forward_b = literal_array(&mut forward_allocation, "b");
        let forward_union =
            forward_allocation.union([forward_b, forward_a], UnionPolicy::Canonical);

        assert_eq!(
            reverse_allocation.display(reverse_union),
            "\"a\"[] | \"b\"[]"
        );
        assert_eq!(
            reverse_allocation.display(reverse_union),
            forward_allocation.display(forward_union)
        );
    }

    #[test]
    fn canonical_union_reduces_literal_families_and_dominant_members() {
        let mut store = TypeStore::new();
        let string_literal = store.intern(TypeKind::LiteralString(
            "value".to_string(),
            LiteralProvenance::Regular,
        ));
        let true_literal = store.intern(TypeKind::LiteralBoolean(true, LiteralProvenance::Regular));
        let false_literal =
            store.intern(TypeKind::LiteralBoolean(false, LiteralProvenance::Regular));
        let never = store.builtins.never;
        let string = store.builtins.string;
        let boolean = store.builtins.boolean;
        let any = store.builtins.any;
        let unknown = store.builtins.unknown;

        assert_eq!(
            store.union([never, string_literal], UnionPolicy::Canonical),
            string_literal
        );
        assert_eq!(
            store.union([string_literal, string], UnionPolicy::Canonical),
            string
        );
        assert_eq!(
            store.union([true_literal, false_literal], UnionPolicy::Canonical),
            boolean
        );
        assert_eq!(
            store.union([string_literal, any], UnionPolicy::Canonical),
            any
        );
        assert_eq!(store.union([any, unknown], UnionPolicy::Canonical), any);
    }

    #[test]
    fn numeric_order_is_value_order_and_authored_structural_order_is_explicit() {
        let mut store = TypeStore::new();
        let ten = store.numeric_literal("10", LiteralProvenance::Regular);
        let two = store.numeric_literal("2", LiteralProvenance::Regular);
        let numeric = store.union([ten, two], UnionPolicy::Canonical);
        assert_eq!(store.display(numeric), "2 | 10");

        let exponent = store.numeric_literal("1e3", LiteralProvenance::Regular);
        let unsafe_integer = store.numeric_literal("9007199254740993", LiteralProvenance::Regular);
        let rounded_integer = store.numeric_literal("9007199254740992", LiteralProvenance::Regular);
        assert_eq!(store.display(exponent), "1000");
        assert_eq!(unsafe_integer, rounded_integer);

        let format_edges = [
            ("0.1", "0.1"),
            ("0.0001", "0.0001"),
            ("1.25", "1.25"),
            ("1e-7", "1e-7"),
            ("1e-6", "0.000001"),
            ("1e20", "100000000000000000000"),
            ("1e21", "1e+21"),
            ("1000000000000000000001", "1e+21"),
        ];
        for (source, expected) in format_edges {
            let literal = store.numeric_literal(source, LiteralProvenance::Regular);
            assert_eq!(store.display(literal), expected, "source: {source}");
        }

        let radix_edges = [
            ("0x20000000000001", "9007199254740992"),
            ("0x20000000000000", "9007199254740992"),
            ("0b1010", "10"),
            ("0o12", "10"),
            ("0x10000000000000000", "18446744073709552000"),
            ("0x20000000000000000", "36893488147419103000"),
            ("0xfffffffffffffffff", "295147905179352830000"),
        ];
        for (source, expected) in radix_edges {
            let literal = store.numeric_literal(source, LiteralProvenance::Regular);
            assert_eq!(store.display(literal), expected, "source: {source}");
        }

        let before_invalid = store.len();
        assert!(
            store
                .try_numeric_literal("not-a-number", LiteralProvenance::Regular)
                .is_err()
        );
        assert_eq!(
            store.numeric_literal("not-a-number", LiteralProvenance::Regular),
            store.builtins.error
        );
        assert_eq!(store.len(), before_invalid);

        let string = store.builtins.string;
        let left = store.object(vec![Property {
            name: "left".to_string(),
            ty: string,
            optional: false,
            readonly: false,
        }]);
        let right = store.object(vec![Property {
            name: "right".to_string(),
            ty: string,
            optional: false,
            readonly: false,
        }]);
        let authored = store.union(
            [right, left, right],
            UnionPolicy::PreserveAuthoredStructuralOrder,
        );
        assert_eq!(
            store.display(authored),
            "{ right: string; } | { left: string; }"
        );
    }
}
