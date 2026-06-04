impl PartialEq for CallableShape {
    fn eq(&self, other: &Self) -> bool {
        // Include symbol in equality check to ensure different classes get different TypeIds
        // The Solver does structural subtyping explicitly, not via PartialEq
        self.call_signatures == other.call_signatures
            && self.construct_signatures == other.construct_signatures
            && self.properties == other.properties
            && index_signature_display_eq(&self.string_index, &other.string_index)
            && index_signature_display_eq(&self.number_index, &other.number_index)
            && self.symbol == other.symbol
            && self.is_abstract == other.is_abstract
    }
}

impl Eq for CallableShape {}

impl std::hash::Hash for CallableShape {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Include the `symbol` field in hash for nominal interning
        // This ensures different classes get different TypeIds
        self.call_signatures.hash(state);
        self.construct_signatures.hash(state);
        self.properties.hash(state);
        hash_index_signature_display(&self.string_index, state);
        hash_index_signature_display(&self.number_index, state);
        self.symbol.hash(state);
        self.is_abstract.hash(state);
    }
}

/// Parameter information
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ParamInfo {
    pub name: Option<Atom>,
    pub type_id: TypeId,
    pub optional: bool,
    pub rest: bool,
}

impl ParamInfo {
    /// Returns `true` if this parameter is required (non-optional, non-rest).
    pub const fn is_required(&self) -> bool {
        !self.optional && !self.rest
    }

    /// Create a required parameter.
    pub const fn required(name: Atom, type_id: TypeId) -> Self {
        Self {
            name: Some(name),
            type_id,
            optional: false,
            rest: false,
        }
    }

    /// Create an optional parameter.
    pub const fn optional(name: Atom, type_id: TypeId) -> Self {
        Self {
            optional: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create a rest parameter.
    pub const fn rest(name: Atom, type_id: TypeId) -> Self {
        Self {
            rest: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create an unnamed required parameter.
    pub const fn unnamed(type_id: TypeId) -> Self {
        Self {
            name: None,
            type_id,
            optional: false,
            rest: false,
        }
    }
}

/// Type parameter information
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeParamInfo {
    pub name: Atom,
    pub constraint: Option<TypeId>,
    pub default: Option<TypeId>,
    /// Whether this is a const type parameter (TS 5.0+)
    /// Const type parameters preserve literal types and infer readonly modifiers
    pub is_const: bool,
}

impl TypeParamInfo {
    /// Unconstrained, non-const type parameter with no default.
    pub const fn simple(name: Atom) -> Self {
        Self {
            name,
            constraint: None,
            default: None,
            is_const: false,
        }
    }
}

/// Reference to a symbol (for named types)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolRef(pub u32);

/// Conditional type structure
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConditionalType {
    pub check_type: TypeId,
    pub extends_type: TypeId,
    pub true_type: TypeId,
    pub false_type: TypeId,
    pub is_distributive: bool,
}

/// Mapped type structure
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MappedType {
    pub type_param: TypeParamInfo,
    pub constraint: TypeId,
    pub name_type: Option<TypeId>,
    pub template: TypeId,
    pub readonly_modifier: Option<MappedModifier>,
    pub optional_modifier: Option<MappedModifier>,
}

impl MappedType {
    /// Resolve the result `readonly`-ness of a homomorphic mapped type applied
    /// to an array/tuple source.
    ///
    /// `+readonly` always makes the result readonly, `-readonly` always makes
    /// it mutable, and an absent modifier copies the source's readonly-ness
    /// (the homomorphic "copy modifiers" behavior `tsc` uses for arrays/tuples,
    /// mirroring per-property modifier copying for object sources).
    pub const fn resolve_readonly(&self, source_readonly: bool) -> bool {
        match self.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => source_readonly,
        }
    }
}

/// Mapped type modifier (+/-)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MappedModifier {
    Add,
    Remove,
}

/// Template literal span
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TemplateSpan {
    Text(Atom),
    Type(TypeId),
}

impl TemplateSpan {
    /// Check if this span is a text span
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Check if this span is a type interpolation
    pub const fn is_type(&self) -> bool {
        matches!(self, Self::Type(_))
    }

    /// Get the text content if this is a text span
    pub const fn as_text(&self) -> Option<Atom> {
        match self {
            Self::Text(atom) => Some(*atom),
            _ => None,
        }
    }

    /// Get the type ID if this is a type span
    pub const fn as_type(&self) -> Option<TypeId> {
        match self {
            Self::Type(type_id) => Some(*type_id),
            _ => None,
        }
    }

    /// Create a type span
    pub const fn type_from_id(type_id: TypeId) -> Self {
        Self::Type(type_id)
    }
}

/// Process escape sequences in a template literal string
/// Handles: \${, \\, \n, \r, \t, \b, \f, \v, \0, \xXX, \uXXXX, \u{X...}
pub fn process_template_escape_sequences(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    let mut last_was_backslash = false;

    while let Some(c) = chars.next() {
        if last_was_backslash {
            last_was_backslash = false;
            match c {
                '$' => {
                    // \$${ becomes $ (not an interpolation)
                    result.push('$');
                }
                '\\' => result.push('\\'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                'b' => result.push('\x08'),
                'f' => result.push('\x0c'),
                'v' => result.push('\x0b'),
                '0' => result.push('\0'),
                'x' => {
                    // \xXX - exactly 2 hex digits
                    let hex1 = chars.next().unwrap_or('0');
                    let hex2 = chars.next().unwrap_or('0');
                    let code = u8::from_str_radix(&format!("{hex1}{hex2}"), 16).unwrap_or(0);
                    result.push(code as char);
                }
                'u' => {
                    // \uXXXX or \u{X...}
                    if let Some('{') = chars.next() {
                        // \u{X...} - Unicode code point
                        let mut code_str = String::new();
                        for nc in chars.by_ref() {
                            if nc == '}' {
                                break;
                            }
                            code_str.push(nc);
                        }
                        if let Ok(code) = u32::from_str_radix(&code_str, 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            result.push(c);
                        }
                    } else {
                        // \uXXXX - exactly 4 hex digits
                        let mut code_str = String::new();
                        for _ in 0..4 {
                            if let Some(nc) = chars.next() {
                                code_str.push(nc);
                            }
                        }
                        if let Ok(code) = u16::from_str_radix(&code_str, 16)
                            && let Some(c) = char::from_u32(code as u32)
                        {
                            result.push(c);
                        }
                    }
                }
                _ => {
                    // Unknown escape - preserve the backslash and character
                    result.push('\\');
                    result.push(c);
                }
            }
        } else if c == '\\' {
            last_was_backslash = true;
        } else {
            result.push(c);
        }
    }

    // Handle trailing backslash
    if last_was_backslash {
        result.push('\\');
    }

    result
}

/// Returns true if the type name corresponds to a built-in type that should
/// be represented structurally or intrinsically, rather than by reference.
///
/// ## Built-in vs Referenced Types
///
/// **Built-in types** (managed by the compiler) are represented directly by their
/// structure (e.g., `TypeData::Array`) rather than by symbol reference (`TypeData::Ref`).
/// This ensures canonicalization: `Array<number>` and `number[]` resolve to the same type.
///
/// **Referenced types** (user-defined and lib types) are represented as `TypeData::Ref(symbol_id)`
/// and resolved lazily during type checking through the `TypeEnvironment`.
///
/// ## Examples
///
/// - `Array<T>` → `TypeData::Array(T)` (structural, not `Ref`)
/// - `Uppercase<S>` → `TypeData::StringIntrinsic { kind: Uppercase, ... }`
/// - `MyInterface` → `TypeData::Ref(SymbolRef(sym_id))`
///
/// ## When to Add Types
///
/// Add a type to this list if:
/// 1. It has special structural representation in `TypeData` (e.g., `Array`)
/// 2. It is a compiler intrinsic (e.g., `Uppercase`, `Lowercase`)
/// 3. It needs canonicalization with alternative syntax (e.g., `T[]` vs `Array<T>`)
///
/// **DO NOT** add:
/// - Regular lib types like `Promise`, `Map`, `Set` (these use `Ref`)
/// - User-defined interfaces or type aliases
pub fn is_compiler_managed_type(name: &str) -> bool {
    matches!(
        name,
        "Array" |          // Canonicalizes with T[] syntax
        "ReadonlyArray" |   // Built-in readonly array type
        "Uppercase" |       // String intrinsic
        "Lowercase" |       // String intrinsic
        "Capitalize" |      // String intrinsic
        "Uncapitalize" // String intrinsic
    )
}

bitflags::bitflags! {
    /// Variance of a type parameter in a generic type.
    ///
    /// Variance determines how subtyping of generic types relates to subtyping
    /// of their type arguments. This is critical for O(1) generic assignability.
    ///
    /// ## Variance Kinds
    ///
    /// - **Covariant** (COVARIANT): T<U> <: T<V> iff U <: V
    ///   - Example: `Array`, `ReadonlyArray`, `Promise`
    /// - Most common for immutable containers
    ///
    /// - **Contravariant** (CONTRAVARIANT): T<U> <: T<V> iff V <: U (reversed)
    ///   - Example: Function parameters (in strict mode)
    /// - Rare in practice, mostly for function types
    ///
    /// - **Invariant** (COVARIANT | CONTRAVARIANT): T<U> <: T<V> iff U === V
    ///   - Example: Mutable properties, `Box<T>` with read/write
    /// - Requires both directions to hold
    ///
    /// - **Independent** (empty): Type parameter not used in variance position
    ///   - Example: Type parameter only used in non-variance positions
    /// - Can be skipped in subtype checks (always compatible)
    ///
    /// ## Examples
    ///
    /// ```typescript
    /// // Covariant: Array< Dog > <: Array< Animal >
    /// type Covariant<T> = { readonly get(): T };
    ///
    /// // Contravariant: Writer< Animal > <: Writer< Dog >
    /// type Contravariant<T> = { write(x: T): void };
    ///
    /// // Invariant: Box<Dog> NOT <: Box<Animal> (mutable!)
    /// type Invariant<T> = { get(): T; set(x: T): void };
    /// ```
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct Variance: u8 {
        /// Covariant position (e.g., function return types)
        const COVARIANT = 1 << 0;
        /// Contravariant position (e.g., function parameters)
        const CONTRAVARIANT = 1 << 1;
        /// Variance may be unreliable due to mapped type modifiers (-?/+?/-readonly/+readonly).
        /// When set, the variance shortcut must fall through to structural comparison
        /// because modifiers can transform mutually-assignable type arguments into
        /// structurally incompatible results (e.g., Required<{a?}> vs Required<{b?}>).
        const NEEDS_STRUCTURAL_FALLBACK = 1 << 2;
        /// Variance-based REJECTION is unreliable. Different type arguments can
        /// produce structurally equivalent instantiations through indexed access
        /// types and intersection normalization. When set, a variance failure
        /// should fall through to structural comparison instead of conclusively
        /// rejecting. Example: `DT<{base: Base, new: New}>` vs
        /// `DT<{base: Base, new: New & Base}>` where `S["base"] & S["new"]`
        /// normalizes to the same type for both.
        const REJECTION_UNRELIABLE = 1 << 3;
        /// The type parameter was found in a direct (non-mapped-type) position,
        /// such as a function parameter, return type, or property type. When set
        /// alongside NEEDS_STRUCTURAL_FALLBACK, the variance rejection can be
        /// trusted because the direct usage provides a reliable variance signal
        /// that dominates over the unreliable mapped-type contribution.
        const DIRECT_USAGE = 1 << 4;
    }
}

impl Variance {
    /// Check if this is an independent type parameter (not used in variance position).
    pub const fn is_independent(&self) -> bool {
        !self.contains(Self::COVARIANT) && !self.contains(Self::CONTRAVARIANT)
    }

    /// Check if this is covariant only.
    pub const fn is_covariant(&self) -> bool {
        self.contains(Self::COVARIANT) && !self.contains(Self::CONTRAVARIANT)
    }

    /// Check if this is contravariant only.
    pub const fn is_contravariant(&self) -> bool {
        self.contains(Self::CONTRAVARIANT) && !self.contains(Self::COVARIANT)
    }

    /// Check if this is invariant (both covariant and contravariant).
    pub fn is_invariant(&self) -> bool {
        self.contains(Self::COVARIANT | Self::CONTRAVARIANT)
    }

    /// Check if variance requires structural fallback (unreliable due to mapped type modifiers).
    pub const fn needs_structural_fallback(&self) -> bool {
        self.contains(Self::NEEDS_STRUCTURAL_FALLBACK)
    }

    /// Check if variance-based rejection is unreliable. When true, a variance
    /// failure should fall through to structural comparison because indexed
    /// access types and intersections can normalize away differences between
    /// type arguments, producing structurally equivalent instantiations.
    pub const fn rejection_unreliable(&self) -> bool {
        self.contains(Self::REJECTION_UNRELIABLE)
    }

    /// Check if the type parameter was found in a direct (non-mapped-type) position.
    /// When true alongside `needs_structural_fallback()`, the variance rejection
    /// is still reliable because the direct usage provides a trustworthy signal.
    pub const fn has_direct_usage(&self) -> bool {
        self.contains(Self::DIRECT_USAGE)
    }

    /// Compose two variances (for nested generics).
    ///
    /// Rules:
    /// - Independent × anything = Independent
    /// - Covariant × Covariant = Covariant
    /// - Covariant × Contravariant = Contravariant
    /// - Contravariant × Covariant = Contravariant
    /// - Contravariant × Contravariant = Covariant
    /// - Invariant × anything = Invariant
    pub fn compose(&self, other: Self) -> Self {
        if self.is_invariant() || other.is_invariant() {
            return Self::COVARIANT | Self::CONTRAVARIANT;
        }
        if self.is_independent() || other.is_independent() {
            return Self::empty();
        }

        // XOR for covariance composition
        let is_covariant = self.is_covariant() == other.is_covariant();
        let is_contravariant = !is_covariant;

        let mut result = Self::empty();
        if is_covariant {
            result |= Self::COVARIANT;
        }
        if is_contravariant {
            result |= Self::CONTRAVARIANT;
        }
        result
    }
}
