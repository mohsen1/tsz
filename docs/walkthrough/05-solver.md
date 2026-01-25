# Solver Module Deep Dive

The solver is the type system core (~142,000 lines), implementing subtyping, inference, instantiation, and type evaluation. It uses semantic subtyping with coinductive semantics for recursive types.

## File Structure

```
src/solver/
├── mod.rs                    Module organization
├── types.rs                  Type representation (TypeKey, TypeId)
├── intern.rs                 Type interning for O(1) equality
├── db.rs                     Query database traits
├── salsa_db.rs               Experimental Salsa integration
├── subtype.rs                Main subtyping algorithm
├── subtype_rules/            Specialized subtype rules
│   ├── unions.rs             Union/intersection rules
│   ├── objects.rs            Object compatibility
│   ├── functions.rs          Function variance
│   ├── intrinsics.rs         Primitive type rules
│   ├── literals.rs           Literal type handling
│   ├── tuples.rs             Array/tuple compatibility
│   ├── generics.rs           Type parameters
│   └── conditionals.rs       Conditional type rules
├── evaluate.rs               Meta-type evaluation
├── evaluate_rules/           Evaluation rule modules
│   ├── conditional.rs        T extends U ? X : Y
│   ├── index_access.rs       T[K]
│   ├── mapped.rs             { [K in T]: V }
│   ├── keyof.rs              keyof T
│   ├── template_literal.rs   `${T}`
│   └── ...
├── infer.rs                  Type inference (Union-Find)
├── instantiate.rs            Generic substitution
├── lower.rs                  AST → TypeId bridge
├── operations.rs             Type operations
├── visitor.rs                Visitor pattern
├── compat.rs                 TypeScript compatibility layer
├── lawyer.rs                 Any-propagation rules
├── tracer.rs                 Zero-cost diagnostics
├── diagnostics.rs            Failure reasons
├── unsoundness_audit.rs      Implementation tracking
└── ...
```

## Core Type Representation

### 📍 KEY: TypeKey Enum (`types.rs`)

```rust
pub enum TypeKey {
    // Primitives
    Intrinsic(IntrinsicKind),       // number, string, boolean, etc.
    Literal(LiteralValue),           // "hello", 42, true

    // Collections
    Array(TypeId),                   // T[]
    Tuple(TupleListId),              // [T, U, ...V]

    // Objects
    Object(ObjectShapeId),           // { prop: T }
    ObjectWithIndex(ObjectShapeId),  // { [key: string]: T }

    // Composites
    Union(TypeListId),               // T | U
    Intersection(TypeListId),        // T & U

    // Functions
    Function(FunctionShapeId),       // (x: T) => U
    Callable(CallableShapeId),       // Overloaded functions

    // Generics
    TypeParameter(TypeParamInfo),    // T (unresolved)
    Infer(TypeParamInfo),            // infer T
    Ref(SymbolRef),                  // Named type reference
    Application(TypeApplicationId),  // Generic<Args>

    // Advanced
    Conditional(ConditionalTypeId),  // T extends U ? X : Y
    Mapped(MappedTypeId),            // { [K in T]: V }
    IndexAccess { object: TypeId, key: TypeId },  // T[K]
    KeyOf(TypeId),                   // keyof T
    TemplateLiteral(TemplateLiteralId),  // `prefix${T}suffix`
    TypeQuery(SymbolRef),            // typeof value
    ThisType,                        // this
    UniqueSymbol(Atom),              // unique symbol
    ReadonlyType(Box<TypeKey>),      // readonly T
    DestructuringPattern,            // For destructuring
}
```

### TypeId

Newtype wrapper enabling O(1) comparison:

```rust
pub struct TypeId(u32);

impl TypeId {
    // Built-in constants
    pub const ANY: TypeId = TypeId(0);
    pub const NEVER: TypeId = TypeId(1);
    pub const UNKNOWN: TypeId = TypeId(2);
    pub const ERROR: TypeId = TypeId(3);
    pub const STRING: TypeId = TypeId(4);
    pub const NUMBER: TypeId = TypeId(5);
    pub const BOOLEAN: TypeId = TypeId(6);
    pub const BIGINT: TypeId = TypeId(7);
    pub const SYMBOL: TypeId = TypeId(8);
    pub const VOID: TypeId = TypeId(9);
    pub const NULL: TypeId = TypeId(10);
    pub const UNDEFINED: TypeId = TypeId(11);
    pub const OBJECT: TypeId = TypeId(12);
    pub const FUNCTION: TypeId = TypeId(13);
}
```

### Type Interning (`intern.rs`)

All types go through the interner for structural deduplication:

```rust
pub struct TypeInterner {
    types: RefCell<IndexSet<TypeKey>>,
    object_shapes: RefCell<Vec<ObjectShape>>,
    function_shapes: RefCell<Vec<FunctionShape>>,
    type_lists: RefCell<Vec<Vec<TypeId>>>,
    // ... more pools
}

impl TypeInterner {
    pub fn intern(&self, key: TypeKey) -> TypeId {
        let mut types = self.types.borrow_mut();
        let (index, _) = types.insert_full(key);
        TypeId(index as u32 + BUILTIN_TYPE_COUNT)
    }

    pub fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        self.types.borrow().get_index(id.0 as usize - BUILTIN_TYPE_COUNT).cloned()
    }
}
```

**Benefits**:
- Same structure = same `TypeId`
- O(1) equality: `type_a == type_b`
- Automatic deduplication
- Efficient caching

## Query Database Layer (`db.rs`)

### TypeDatabase Trait (`db.rs`)

Low-level interning and lookup:

```rust
pub trait TypeDatabase {
    fn intern(&self, key: TypeKey) -> TypeId;
    fn lookup(&self, id: TypeId) -> Option<TypeKey>;

    // Shape accessors
    fn object_shape(&self, id: ObjectShapeId) -> &ObjectShape;
    fn function_shape(&self, id: FunctionShapeId) -> &FunctionShape;
    fn type_list(&self, id: TypeListId) -> &[TypeId];
    // ...
}
```

### QueryDatabase Trait (`db.rs`)

Higher-level type operations:

```rust
pub trait QueryDatabase: TypeDatabase {
    fn evaluate_type(&self, id: TypeId) -> TypeId;
    fn evaluate_conditional(&self, id: ConditionalTypeId) -> TypeId;
    fn evaluate_index_access(&self, object: TypeId, key: TypeId) -> TypeId;
    fn evaluate_mapped(&self, id: MappedTypeId) -> TypeId;
    fn evaluate_keyof(&self, id: TypeId) -> TypeId;
    fn resolve_property_access(&self, object: TypeId, name: &str) -> Option<TypeId>;
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool;
    fn new_inference_context(&self) -> InferenceContext;
}
```

## Subtyping Algorithm (`subtype.rs`)

### 📍 KEY: Coinductive Semantics

The solver uses Greatest Fixed Point semantics for recursive types:

```rust
// subtype.rs
pub struct SubtypeChecker<'a> {
    interner: &'a dyn TypeDatabase,
    in_progress: HashSet<(TypeId, TypeId)>,  // Cycle detection
    depth: u32,
    total_checks: u32,
}

impl SubtypeChecker<'_> {
    pub fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
        // Cycle detection: if we've seen this pair, assume true
        if self.in_progress.contains(&(source, target)) {
            return SubtypeResult::Provisional;  // Break cycle
        }

        // Mark as in-progress
        self.in_progress.insert((source, target));

        // ... structural checking ...

        // Remove from in-progress
        self.in_progress.remove(&(source, target));

        result
    }
}
```

### SubtypeResult

```rust
pub enum SubtypeResult {
    True,        // Definitely a subtype
    False,       // Definitely not a subtype
    Provisional, // In a cycle (assuming true)
}
```

### Fast Paths (`subtype.rs`)

```rust
fn check_subtype(&mut self, source: TypeId, target: TypeId) -> SubtypeResult {
    // Identity
    if source == target {
        return SubtypeResult::True;
    }

    // Top types
    if source == TypeId::ANY || target == TypeId::ANY {
        return SubtypeResult::True;
    }
    if target == TypeId::UNKNOWN {
        return SubtypeResult::True;
    }

    // Bottom type
    if source == TypeId::NEVER {
        return SubtypeResult::True;
    }

    // ... structural checks
}
```

### Critical Limits

```rust
const MAX_SUBTYPE_DEPTH: u32 = 100;           // Recursion limit
const MAX_TOTAL_SUBTYPE_CHECKS: u32 = 100_000; // Total checks per instance
const MAX_IN_PROGRESS_PAIRS: usize = 10_000;   // Memory safety
```

## Subtype Rules

### Union Rules (`subtype_rules/unions.rs`)

```rust
// Union source: ALL members must be subtypes of target
// S₁ | S₂ <: T  ⟺  S₁ <: T ∧ S₂ <: T
fn check_union_source(&mut self, source_types: &[TypeId], target: TypeId) -> SubtypeResult {
    for source_member in source_types {
        if self.check_subtype(*source_member, target) == SubtypeResult::False {
            return SubtypeResult::False;
        }
    }
    SubtypeResult::True
}

// Union target: source must be subtype of AT LEAST ONE member
// S <: T₁ | T₂  ⟺  S <: T₁ ∨ S <: T₂
fn check_union_target(&mut self, source: TypeId, target_types: &[TypeId]) -> SubtypeResult {
    for target_member in target_types {
        if self.check_subtype(source, *target_member) == SubtypeResult::True {
            return SubtypeResult::True;
        }
    }
    SubtypeResult::False
}
```

### Object Rules (`subtype_rules/objects.rs`)

```rust
fn check_object_subtype(
    &mut self,
    source: &ObjectShape,
    target: &ObjectShape,
) -> SubtypeResult {
    // For each target property, source must have compatible property
    for target_prop in &target.properties {
        let source_prop = source.properties.iter()
            .find(|p| p.name == target_prop.name);

        match source_prop {
            Some(prop) => {
                // Property type compatibility
                if self.check_subtype(prop.ty, target_prop.ty) == SubtypeResult::False {
                    return SubtypeResult::False;
                }
            }
            None => {
                // Missing property - check if optional
                if !target_prop.optional {
                    return SubtypeResult::False;
                }
            }
        }
    }

    SubtypeResult::True
}
```

### 📍 KEY: Split Accessor Variance (Rule #26)

```rust
// Getters and setters have different variance:
// - Read types: covariant (source.read <: target.read)
// - Write types: contravariant (target.write <: source.write)

fn check_accessor_compatibility(&mut self, source: &PropertyInfo, target: &PropertyInfo) -> SubtypeResult {
    // Read type (getter) - covariant
    if let (Some(source_read), Some(target_read)) = (&source.read_type, &target.read_type) {
        if self.check_subtype(*source_read, *target_read) == SubtypeResult::False {
            return SubtypeResult::False;
        }
    }

    // Write type (setter) - contravariant
    if let (Some(source_write), Some(target_write)) = (&source.write_type, &target.write_type) {
        if self.check_subtype(*target_write, *source_write) == SubtypeResult::False {
            return SubtypeResult::False;
        }
    }

    SubtypeResult::True
}
```

### Function Rules (`subtype_rules/functions.rs`)

```rust
fn check_function_subtype(
    &mut self,
    source: &FunctionShape,
    target: &FunctionShape,
) -> SubtypeResult {
    // Return type: covariant
    // source returns <: target returns
    if self.check_subtype(source.return_type, target.return_type) == SubtypeResult::False {
        return SubtypeResult::False;
    }

    // Parameters: contravariant (strict mode) or bivariant (legacy)
    for (source_param, target_param) in source.parameters.iter().zip(&target.parameters) {
        if self.strict_function_types {
            // Contravariant: target_type <: source_type
            if self.check_subtype(target_param.ty, source_param.ty) == SubtypeResult::False {
                return SubtypeResult::False;
            }
        } else {
            // Bivariant: either direction works (unsound but TypeScript-compatible)
            let forward = self.check_subtype(source_param.ty, target_param.ty);
            let backward = self.check_subtype(target_param.ty, source_param.ty);
            if forward == SubtypeResult::False && backward == SubtypeResult::False {
                return SubtypeResult::False;
            }
        }
    }

    SubtypeResult::True
}
```

## Type Inference (`infer.rs`)

### Union-Find via `ena` crate

```rust
pub struct InferenceContext {
    table: InPlaceUnificationTable<InferenceVar>,
    constraints: FxHashMap<InferenceVar, ConstraintSet>,
}

pub struct ConstraintSet {
    pub lower_bounds: Vec<TypeId>,  // L <: α
    pub upper_bounds: Vec<TypeId>,  // α <: U
}
```

### Constraint Collection

```rust
impl InferenceContext {
    pub fn add_lower_bound(&mut self, var: InferenceVar, bound: TypeId) {
        self.constraints.entry(var).or_default().lower_bounds.push(bound);
    }

    pub fn add_upper_bound(&mut self, var: InferenceVar, bound: TypeId) {
        self.constraints.entry(var).or_default().upper_bounds.push(bound);
    }

    pub fn unify(&mut self, a: InferenceVar, b: InferenceVar) {
        self.table.union(a, b);
    }
}
```

### Constraint Solving

```rust
pub fn solve(&mut self, interner: &dyn TypeDatabase) -> Result<Substitution, InferenceError> {
    let mut substitution = FxHashMap::default();

    for (var, constraints) in &self.constraints {
        // Check for conflicts
        if let Some(conflict) = self.detect_conflict(&constraints) {
            return Err(InferenceError::Conflict(conflict.0, conflict.1));
        }

        // Compute best type satisfying bounds
        let inferred = self.compute_best_type(interner, &constraints);
        substitution.insert(*var, inferred);
    }

    Ok(Substitution { map: substitution })
}
```

## Generic Instantiation (`instantiate.rs`)

### TypeSubstitution

```rust
pub struct TypeSubstitution {
    map: FxHashMap<Atom, TypeId>,  // Type parameter name → concrete type
}

impl TypeSubstitution {
    pub fn from_type_arguments(
        params: &[TypeParamInfo],
        args: &[TypeId],
        interner: &dyn TypeDatabase,
    ) -> Self {
        let mut map = FxHashMap::default();

        for (i, param) in params.iter().enumerate() {
            let arg = args.get(i).copied()
                .or(param.default)  // Use default if no arg
                .unwrap_or(TypeId::ANY);
            map.insert(param.name, arg);
        }

        TypeSubstitution { map }
    }
}
```

### TypeInstantiator

```rust
pub struct TypeInstantiator<'a> {
    interner: &'a dyn TypeDatabase,
    substitution: &'a TypeSubstitution,
    visited: RefCell<FxHashSet<TypeId>>,
    depth: RefCell<u32>,
}

impl TypeInstantiator<'_> {
    pub fn substitute(&self, type_id: TypeId) -> TypeId {
        // Depth limit
        if *self.depth.borrow() > MAX_INSTANTIATION_DEPTH {
            return TypeId::ERROR;
        }

        // Cycle detection
        if self.visited.borrow().contains(&type_id) {
            return type_id;  // Return original to break cycle
        }

        *self.depth.borrow_mut() += 1;
        self.visited.borrow_mut().insert(type_id);

        let result = match self.interner.lookup(type_id) {
            Some(TypeKey::TypeParameter(info)) => {
                // Substitute type parameter
                self.substitution.map.get(&info.name)
                    .copied()
                    .unwrap_or(type_id)
            }
            Some(TypeKey::Union(list_id)) => {
                // Recursively substitute in union members
                let types = self.interner.type_list(list_id);
                let substituted: Vec<_> = types.iter()
                    .map(|t| self.substitute(*t))
                    .collect();
                self.interner.intern_union(&substituted)
            }
            // ... more cases
            _ => type_id,
        };

        self.visited.borrow_mut().remove(&type_id);
        *self.depth.borrow_mut() -= 1;

        result
    }
}
```

## Type Lowering (`lower.rs`)

### AST → TypeId Bridge

```rust
pub struct TypeLowering<'a, R: TypeResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: Option<&'a R>,
    type_param_scope: RefCell<Vec<Vec<(Atom, TypeId)>>>,
    operations: RefCell<u32>,  // MAX: 100,000
}

impl TypeLowering<'_, _> {
    pub fn lower_type_node(&self, arena: &NodeArena, node: NodeIndex) -> TypeId {
        match arena.get(node).kind {
            // Keywords
            NumberKeyword => TypeId::NUMBER,
            StringKeyword => TypeId::STRING,
            BooleanKeyword => TypeId::BOOLEAN,
            AnyKeyword => TypeId::ANY,
            NeverKeyword => TypeId::NEVER,
            UnknownKeyword => TypeId::UNKNOWN,
            VoidKeyword => TypeId::VOID,
            NullKeyword => TypeId::NULL,
            UndefinedKeyword => TypeId::UNDEFINED,

            // Composites
            UnionType => {
                let types: Vec<_> = get_type_elements(arena, node)
                    .map(|t| self.lower_type_node(arena, t))
                    .collect();
                self.interner.intern_union(&types)
            }
            IntersectionType => {
                let types: Vec<_> = get_type_elements(arena, node)
                    .map(|t| self.lower_type_node(arena, t))
                    .collect();
                self.interner.intern_intersection(&types)
            }

            // References
            TypeReference => self.lower_type_reference(arena, node),

            // Functions
            FunctionType => self.lower_function_type(arena, node),

            // Advanced
            ConditionalType => self.lower_conditional_type(arena, node),
            MappedType => self.lower_mapped_type(arena, node),
            IndexedAccessType => self.lower_indexed_access_type(arena, node),

            // ... more cases
        }
    }
}
```

## Meta-Type Evaluation (`evaluate.rs`)

### TypeEvaluator

```rust
pub struct TypeEvaluator<'a, R: TypeResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    cache: RefCell<FxHashMap<TypeId, TypeId>>,
    visiting: RefCell<FxHashSet<TypeId>>,
    depth: RefCell<u32>,
    total_evaluations: RefCell<u32>,
}

const MAX_EVALUATE_DEPTH: u32 = 50;
const MAX_TOTAL_EVALUATIONS: u32 = 100_000;
```

### Conditional Type Evaluation (`evaluate_rules/conditional.rs`)

```rust
// T extends U ? X : Y
fn evaluate_conditional(&self, cond: &ConditionalTypeData) -> TypeId {
    // 1. Distributive evaluation for naked type parameters
    if is_naked_type_parameter(cond.check_type) {
        if let TypeKey::Union(types) = self.lookup(cond.check_type) {
            // Distribute: (A | B) extends U ? X : Y
            //           = (A extends U ? X : Y) | (B extends U ? X : Y)
            let results: Vec<_> = types.iter()
                .map(|t| self.evaluate_conditional_with_check(*t, cond))
                .collect();
            return self.intern_union(&results);
        }
    }

    // 2. Check if check_type extends extends_type
    if self.is_subtype_of(cond.check_type, cond.extends_type) {
        self.evaluate(cond.true_type)
    } else {
        self.evaluate(cond.false_type)
    }
}
```

### Mapped Type Evaluation (`evaluate_rules/mapped.rs`)

```rust
// { [K in Keys]: V }
fn evaluate_mapped(&self, mapped: &MappedTypeData) -> TypeId {
    // 1. Evaluate keyof to get all keys
    let keys = self.evaluate_keyof(mapped.constraint_type);

    // 2. Extract string literal keys
    let key_strings = self.extract_string_literals(keys);

    // 3. Build result object
    let mut properties = Vec::new();
    for key in key_strings {
        // Substitute K with current key
        let value_type = self.substitute_type_parameter(
            mapped.template_type,
            mapped.type_parameter,
            key,
        );

        // Apply modifiers: +readonly, -readonly, +?, -?
        let prop = PropertyInfo {
            name: key,
            ty: value_type,
            optional: apply_optional_modifier(mapped.optional_modifier),
            readonly: apply_readonly_modifier(mapped.readonly_modifier),
        };
        properties.push(prop);
    }

    self.intern_object(ObjectShape { properties, .. })
}
```

### Keyof Evaluation (`evaluate_rules/keyof.rs`)

```rust
// keyof T
fn evaluate_keyof(&self, type_id: TypeId) -> TypeId {
    match self.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) => {
            // Get all property names as string literals
            let shape = self.object_shape(shape_id);
            let keys: Vec<_> = shape.properties.iter()
                .map(|p| self.intern_string_literal(&p.name))
                .collect();
            self.intern_union(&keys)
        }
        Some(TypeKey::Union(types)) => {
            // keyof (A | B) = keyof A & keyof B
            let keyofs: Vec<_> = types.iter()
                .map(|t| self.evaluate_keyof(*t))
                .collect();
            self.intern_intersection(&keyofs)
        }
        Some(TypeKey::Intersection(types)) => {
            // keyof (A & B) = keyof A | keyof B
            let keyofs: Vec<_> = types.iter()
                .map(|t| self.evaluate_keyof(*t))
                .collect();
            self.intern_union(&keyofs)
        }
        _ => TypeId::NEVER,
    }
}
```

## Compatibility Layer (`compat.rs`)

### CompatChecker

Bridges structural subtyping with TypeScript's unsound rules:

```rust
pub struct CompatChecker<'a> {
    interner: &'a dyn TypeDatabase,
    subtype_checker: SubtypeChecker<'a>,

    // Configuration
    strict_function_types: bool,
    strict_null_checks: bool,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,

    // Caching
    cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
}
```

### Layered Design

```
                CompatChecker
                     ↓
        AnyPropagationRules ("Lawyer")
                     ↓
            SubtypeChecker (Structural)
```

### Configuration Flags

| Flag | Effect |
|------|--------|
| `strict_function_types` | Enable contravariance for function parameters |
| `strict_null_checks` | Disallow null/undefined in non-null context |
| `no_unchecked_indexed_access` | Include undefined in `T[K]` |
| `exact_optional_property_types` | Distinguish `{k?: T}` from `{k: T \| undefined}` |

## Visitor Pattern (`visitor.rs`)

Alternative to repetitive match statements:

```rust
pub trait TypeVisitor {
    // Required
    fn visit_intrinsic(&mut self, kind: IntrinsicKind);
    fn visit_literal(&mut self, value: &LiteralValue);

    // Optional (have default implementations)
    fn visit_union(&mut self, types: &[TypeId]) {
        for ty in types {
            self.visit(*ty);
        }
    }
    fn visit_object(&mut self, shape: &ObjectShape) { /* ... */ }
    fn visit_function(&mut self, shape: &FunctionShape) { /* ... */ }
    // ... more
}

// Usage:
struct TypeCollector {
    types: Vec<TypeId>,
}

impl TypeVisitor for TypeCollector {
    fn visit_intrinsic(&mut self, _: IntrinsicKind) {}
    fn visit_literal(&mut self, _: &LiteralValue) {}
    fn visit_type_parameter(&mut self, info: &TypeParamInfo) {
        self.types.push(info.id);
    }
}
```

## Unsoundness Audit (`unsoundness_audit.rs`)

### Implementation Status

44 TypeScript unsoundness rules tracked:

| Phase | Rules | Status |
|-------|-------|--------|
| Phase 1 (Hello World) | 5 | ✅ 5/5 implemented |
| Phase 2 (Business Logic) | 5 | ⚠️ 3/5 full, 2/5 partial |
| Phase 3 (Library) | 5 | ⚠️ 3/5 full, 2/5 partial |
| Phase 4 (Feature) | 29 | ⚠️ 17/29 full, 5/29 partial, 7/29 missing |

**Overall**: ~73% coverage (28/44 full, 9/44 partial, 7/44 missing)

### Key Implemented Rules

| Rule | Description | Location |
|------|-------------|----------|
| Any Type | `any` bypasses checking | `compat.rs` |
| Error Poisoning | Unions with `any` simplified | `intern.rs` |
| Covariant Arrays | `T[] <: U[]` when `T <: U` | `subtype_rules/tuples.rs` |
| Function Bivariance | Legacy parameter bivariance | `subtype_rules/functions.rs` |
| Literal Widening | `"foo"` widens to `string` | `subtype_rules/literals.rs` |
| Split Accessor Variance | Get/set have different variance | `subtype_rules/objects.rs` |
| Distributivity Disabling | `[T] extends [U]` not distributed | `evaluate_rules/conditional.rs` |

## Known Gaps

### ✅ FIXED: Freshness/Excess Property Checks (Rule #4)

FreshnessTracker is now integrated with excess property checking in `check_object_literal_excess_properties`.
Only fresh object literals (direct object literal expressions) trigger excess property errors.

### ✅ FIXED: Keyof Contravariance (Rule #30)

Union inversion is correctly implemented in `evaluate_rules/keyof.rs`:
- `keyof (A | B) = (keyof A) & (keyof B)` - distributive contravariance
- `keyof (A & B) = (keyof A) | (keyof B)` - covariance

### ✅ FIXED: Array-to-Tuple Rejection (Rule #15)

Array-to-tuple rejection is correctly implemented in `subtype_rules/tuples.rs`:
- Arrays (`T[]`) are NOT assignable to tuple types
- Exception: `never[]` can be assigned to tuples that allow empty

### ⚠️ GAP: CFA Invalidation in Closures (Rule #42)

```rust
// Not implemented - closures may see stale narrowing
function f(x: string | null) {
    if (x !== null) {
        setTimeout(() => {
            x.length;  // x might be null now
        }, 100);
    }
    x = null;
}
```

### ✅ FIXED: Tracer Module (`mod.rs:37`)

The tracer module is now enabled and working. Fixed type mismatches:
- Updated function/tuple/object parameter types to use shape IDs
- Fixed union/intersection to use TypeListId
- Corrected intrinsic subtype checking for `any <: never`

### ⚠️ GAP: Template Literal Cross-Product (Rule #38)

```rust
const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;
// No correlated access optimization for large unions
```

**Impact**: Large template literal types may hit expansion limit

---

**Previous**: [04-checker.md](./04-checker.md) - Checker Module
**Next**: [06-emitter.md](./06-emitter.md) - Emitter Module
