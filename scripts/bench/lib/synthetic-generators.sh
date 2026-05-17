#!/usr/bin/env bash
#
# Synthetic TypeScript fixture generators shared by:
# - scripts/bench/bench-vs-tsgo.sh        (full + project benchmark suite)
# - scripts/bench/precommit-microbench.sh (lightweight regression gate)
#
# Each function writes a self-contained .ts file to its second argument.
# Generators must not depend on caller-side globals. They communicate by:
#  - first arg: a size knob (count/depth)
#  - second arg: output path
# Some generators read only a single arg (output path) when they are not
# parameterised.
#
# Hotspot coverage (one generator per known scaling axis):
#   classes / interfaces / methods               generate_synthetic_file
#   generics + conditional types                 generate_complex_file
#   DeepPartial mapped + optional chain          generate_deeppartial_optional_chain_file
#   shallow optional-chain (DeepPartial control) generate_shallow_optional_chain_file
#   typed array intrinsics                       generate_typed_arrays_file
#   discriminated unions + exhaustive switch     generate_union_file
#   recursive generic instantiation              generate_recursive_generic_file
#   conditional type distribution                generate_conditional_distribution_file
#   mapped type expansion                        generate_mapped_type_file
#   template literal cartesian product           generate_template_literal_file
#   deep recursive subtype                       generate_deep_subtype_file
#   intersection normalization                   generate_intersection_file
#   infer keyword + Parameters/ReturnType        generate_infer_stress_file
#   control flow analysis (switch + if-chains)   generate_cfa_stress_file
#   best common type (BCT)                       generate_bct_stress_file
#   constraint conflict detection                generate_constraint_conflict_file
#   mapped types with complex templates          generate_mapped_complex_template_file
#   deeply nested keyof / indexed access         generate_keyof_chain_file
#   function overload resolution                 generate_overload_resolution_file
#   wide object-literal assignment scan          generate_object_literal_assign_file
#   indexed access over mapped readers           generate_indexed_access_hotspot_file
#   remapped accessor (get/set) mapped type      generate_remapped_accessor_hotspot_file
#   conditional + infer extraction chains        generate_conditional_infer_hotspot_file
#   object spread inference + property merging   generate_object_spread_hotspot_file
#   contextual callback dispatch tables          generate_contextual_callback_hotspot_file

generate_synthetic_file() {
    local class_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Synthetic TypeScript benchmark file
// Auto-generated for performance testing

HEADER

    for ((i=0; i<class_count; i++)); do
        cat >> "$output" << EOF
export interface Config$i {
    readonly id: number;
    name: string;
    enabled: boolean;
    options?: Record<string, unknown>;
}

export class Service$i implements Config$i {
    readonly id: number = $i;
    name: string;
    enabled: boolean = true;
    private items: string[] = [];

    constructor(name: string) {
        this.name = name;
    }

    getId(): number {
        return this.id;
    }

    getName(): string {
        return this.name;
    }

    setName(value: string): void {
        this.name = value;
    }

    isEnabled(): boolean {
        return this.enabled;
    }

    addItem(item: string): void {
        this.items.push(item);
    }

    getItems(): readonly string[] {
        return this.items;
    }

    static create(name: string): Service$i {
        return new Service$i(name);
    }
}

EOF
    done
}

generate_complex_file() {
    local func_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Complex TypeScript with generics, unions, and conditional types
/// <reference lib="es2015.promise" />

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;

interface Result<T, E = Error> {
    ok: boolean;
    value?: T;
    error?: E;
}

HEADER

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
async function process$i<T extends Record<string, unknown>>(
    input: T,
    options?: DeepPartial<{ timeout: number; retries: number }>
): Promise<Result<T>> {
    const timeout = options?.timeout ?? 1000;
    const retries = options?.retries ?? 3;
    
    for (let attempt = 0; attempt < retries; attempt++) {
        try {
            const result = await Promise.resolve(input);
            if (timeout < 0) {
                throw new Error('timeout');
            }
            return { ok: true, value: result };
        } catch (e) {
            if (attempt === retries - 1) {
                return { ok: false, error: e as Error };
            }
        }
    }
    return { ok: false, error: new Error('exhausted') };
}

EOF
    done
}

generate_deeppartial_optional_chain_file() {
    local func_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// DeepPartial + optional-chain hotspot benchmark.
// This isolates recursive mapped-type expansion on repeated property access.

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type Normalize<T> = T extends object ? { [P in keyof T]: Normalize<T[P]> } : T;
type DeepInput<T> = DeepPartial<Normalize<T>>;

interface RetryOptions {
    timeout: number;
    retries: number;
    nested: {
        transport: {
            backoff: {
                base: number;
                max: number;
                jitter: number;
            };
        };
        flags: {
            fast: boolean;
            safe: boolean;
        };
    };
}
HEADER

    local score_expr='(options?.timeout ?? 1000) + (options?.nested?.transport?.backoff?.base ?? 10) + (options?.nested?.transport?.backoff?.max ?? 100) + (options?.nested?.transport?.backoff?.jitter ?? 1) + (options?.nested?.flags?.safe ? 1 : 0) + (options?.nested?.flags?.fast ? 1 : 0) + (options?.retries ?? 3)'

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
function deepPartialHotspot$i(
    options?: DeepInput<RetryOptions>
): number {
    let score = 0;
EOF
        for ((j=0; j<34; j++)); do
            printf '    score += %s;\n' "$score_expr" >> "$output"
        done
        cat >> "$output" << 'EOF'
    return score;
}

EOF
    done
}

generate_shallow_optional_chain_file() {
    local func_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Shallow optional-chain control benchmark.
// Same structure as DeepPartial hotspot but without recursive mapped types.

interface RetryOptionsShallow {
    timeout?: number;
    retries?: number;
    nested?: {
        transport?: {
            backoff?: {
                base?: number;
                max?: number;
                jitter?: number;
            };
        };
        flags?: {
            fast?: boolean;
            safe?: boolean;
        };
    };
}
HEADER

    local score_expr='(options?.timeout ?? 1000) + (options?.nested?.transport?.backoff?.base ?? 10) + (options?.nested?.transport?.backoff?.max ?? 100) + (options?.nested?.transport?.backoff?.jitter ?? 1) + (options?.nested?.flags?.safe ? 1 : 0) + (options?.nested?.flags?.fast ? 1 : 0) + (options?.retries ?? 3)'

    for ((i=0; i<func_count; i++)); do
        cat >> "$output" << EOF
function shallowOptionalControl$i(
    options?: RetryOptionsShallow
): number {
    let score = 0;
EOF
        for ((j=0; j<34; j++)); do
            printf '    score += %s;\n' "$score_expr" >> "$output"
        done
        cat >> "$output" << 'EOF'
    return score;
}

EOF
    done
}
generate_typed_arrays_file() {
    local output="$1"

    cat > "$output" << 'HEADER'
// Typed array benchmark fixture used by bench-vs-tsgo.sh.
// Keep this strict/explicit so all compilers can parse and type-check it.

function createTypedArrayInstancesFromLength(length: number) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(length);
    typedArrays[1] = new Uint8Array(length);
    typedArrays[2] = new Int16Array(length);
    typedArrays[3] = new Uint16Array(length);
    typedArrays[4] = new Int32Array(length);
    typedArrays[5] = new Uint32Array(length);
    typedArrays[6] = new Float32Array(length);
    typedArrays[7] = new Float64Array(length);
    typedArrays[8] = new Uint8ClampedArray(length);
    return typedArrays;
}

function createTypedArrayInstancesFromArrayLike(obj: ArrayLike<number>) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    typedArrays[1] = new Uint8Array(obj);
    typedArrays[2] = new Int16Array(obj);
    typedArrays[3] = new Uint16Array(obj);
    typedArrays[4] = new Int32Array(obj);
    typedArrays[5] = new Uint32Array(obj);
    typedArrays[6] = new Float32Array(obj);
    typedArrays[7] = new Float64Array(obj);
    typedArrays[8] = new Uint8ClampedArray(obj);
    return typedArrays;
}

function createTypedArraysFromMapFn(
    obj: ArrayLike<number>,
    mapFn: (n: number, v: number) => number
) {
    const typedArrays = [];
    typedArrays[0] = Int8Array.from(obj, mapFn);
    typedArrays[1] = Uint8Array.from(obj, mapFn);
    typedArrays[2] = Int16Array.from(obj, mapFn);
    typedArrays[3] = Uint16Array.from(obj, mapFn);
    typedArrays[4] = Int32Array.from(obj, mapFn);
    typedArrays[5] = Uint32Array.from(obj, mapFn);
    typedArrays[6] = Float32Array.from(obj, mapFn);
    typedArrays[7] = Float64Array.from(obj, mapFn);
    typedArrays[8] = Uint8ClampedArray.from(obj, mapFn);
    return typedArrays;
}

const values: number[] = [1, 2, 3, 4];
const mapped = createTypedArraysFromMapFn(values, (n, i) => n + i);
const fromLength = createTypedArrayInstancesFromLength(128);
const fromArrayLike = createTypedArrayInstancesFromArrayLike(values);
const sampleCount = mapped.length + fromLength.length + fromArrayLike.length;
HEADER
}

generate_union_file() {
    local member_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Union type stress test - discriminated unions with many members

HEADER

    # Generate union type
    echo "type StressEvent =" >> "$output"
    for ((i=0; i<member_count; i++)); do
        if [ $i -eq $((member_count - 1)) ]; then
            echo "    | { type: 'event$i'; payload$i: string; timestamp: number };" >> "$output"
        else
            echo "    | { type: 'event$i'; payload$i: string; timestamp: number }" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    
    # Generate handler function with exhaustive switch
    cat >> "$output" << 'HANDLER_START'
function handleEvent(event: StressEvent): string {
    switch (event.type) {
HANDLER_START

    for ((i=0; i<member_count; i++)); do
        echo "        case 'event$i': return event.payload$i;" >> "$output"
    done
    
    cat >> "$output" << 'HANDLER_END'
        default:
            throw new Error('unreachable');
    }
}

HANDLER_END

    # Generate some type narrowing tests
    for ((i=0; i<member_count; i+=10)); do
        cat >> "$output" << EOF
function isEvent$i(e: StressEvent): e is Extract<StressEvent, { type: 'event$i' }> {
    return e.type === 'event$i';
}

EOF
    done
}

# =============================================================================
# SOLVER STRESS TEST GENERATORS
# =============================================================================
# These generators create files that stress specific solver limits defined in
# src/limits.rs. They push close to (but under) hard limits to find perf cliffs.

# Stress: MAX_INSTANTIATION_DEPTH (50), MAX_SUBTYPE_DEPTH (100)
generate_recursive_generic_file() {
    local depth="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Recursive generic type instantiation stress test
// Pushes MAX_INSTANTIATION_DEPTH and subtype checking limits

type LinkedList<T> = { value: T; next: LinkedList<T> | null };
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;

HEADER

    # Generate recursive wrapper types
    for ((i=0; i<depth; i++)); do
        echo "type Wrap$i<T> = { layer$i: T };" >> "$output"
    done
    
    # Generate deeply nested instantiation
    echo "" >> "$output"
    echo "// Deep instantiation chain" >> "$output"
    local chain="string"
    local max_chain=$((depth < 40 ? depth : 40))
    for ((i=max_chain-1; i>=0; i--)); do
        chain="Wrap$i<$chain>"
    done
    echo "type DeepWrapped = $chain;" >> "$output"
    
    # Force evaluation with assignments
    echo "" >> "$output"
    echo "declare const deep: DeepWrapped;" >> "$output"
    echo "declare function extract<T>(x: Wrap0<T>): T;" >> "$output"
    echo "const _test = extract(deep);" >> "$output"
    
    # Add recursive type checks
    echo "" >> "$output"
    echo "// Recursive list operations" >> "$output"
    echo "declare const list: LinkedList<number>;" >> "$output"
    echo "declare function mapList<T, U>(l: LinkedList<T>, f: (x: T) => U): LinkedList<U>;" >> "$output"
    echo "const mapped = mapList(list, x => x.toString());" >> "$output"
}

# Stress: MAX_DISTRIBUTION_SIZE (100), MAX_EVALUATE_DEPTH (50)
generate_conditional_distribution_file() {
    local member_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Conditional type distribution stress test
// Tests large union distribution in conditional types

type ExtractString<T> = T extends string ? T : never;
type ExtractNumber<T> = T extends number ? T : never;
type ExtractArrayType<T> = T extends (infer U)[] ? U : never;
type ToArray<T> = T extends any ? T[] : never;
type Flatten<T> = T extends (infer U)[] ? Flatten<U> : T;

HEADER

    # Generate a large union type
    echo "type BigUnion =" >> "$output"
    for ((i=0; i<member_count; i++)); do
        if [ $i -eq $((member_count - 1)) ]; then
            echo "    | 'value$i';" >> "$output"
        else
            echo "    | 'value$i'" >> "$output"
        fi
    done
    
    # Apply conditional types that distribute over the union
    echo "" >> "$output"
    echo "// Distributive conditional type applications" >> "$output"
    echo "type Distributed1 = ToArray<BigUnion>;" >> "$output"
    echo "type Distributed2 = ExtractString<BigUnion | number>;" >> "$output"
    
    # Chain multiple conditional transformations
    cat >> "$output" << 'EOF'

type ChainedConditional<T> =
    T extends string ? `prefix_${T}` :
    T extends number ? T :
    T extends boolean ? (T extends true ? 1 : 0) :
    never;

type Applied = ChainedConditional<BigUnion>;

// Nested conditional
type NestedConditional<T> =
    T extends `value${infer N}` ? N extends `${infer D}${infer Rest}` ? D : never : never;

type Extracted = NestedConditional<BigUnion>;

EOF

    # Force type evaluation with declarations
    echo "declare const distributed: Distributed1;" >> "$output"
    echo "declare const applied: Applied;" >> "$output"
    echo "declare const extracted: Extracted;" >> "$output"
}

# Stress: MAX_MAPPED_KEYS (500)
generate_mapped_type_file() {
    local key_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Mapped type expansion stress test
// Tests MAX_MAPPED_KEYS limit and mapped type evaluation

type MyOptional<T> = { [K in keyof T]?: T[K] };
type MyRequired<T> = { [K in keyof T]-?: T[K] };
type MyReadonly<T> = { readonly [K in keyof T]: T[K] };
type MyMutable<T> = { -readonly [K in keyof T]: T[K] };

// Advanced mapped types
type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };
type Setters<T> = { [K in keyof T as `set${Capitalize<string & K>}`]: (val: T[K]) => void };

HEADER

    # Generate a type with many properties
    echo "interface BigObject {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        echo "    prop$i: string;" >> "$output"
    done
    echo "}" >> "$output"
    
    # Apply various mapped type transformations
    echo "" >> "$output"
    echo "// Mapped type transformations" >> "$output"
    echo "type Partial1 = MyOptional<BigObject>;" >> "$output"
    echo "type Readonly1 = MyReadonly<BigObject>;" >> "$output"
    echo "type Both = MyReadonly<MyOptional<BigObject>>;" >> "$output"
    echo "" >> "$output"
    echo "type BigGetters = Getters<BigObject>;" >> "$output"
    echo "type BigSetters = Setters<BigObject>;" >> "$output"
    
    # Nested mapped type
    cat >> "$output" << 'EOF'

// Nested mapped type
type DeepOptional<T> = T extends object ? { [K in keyof T]?: DeepOptional<T[K]> } : T;
type DeepBigObject = DeepOptional<BigObject>;

EOF

    # Force evaluation
    echo "declare const partial: Partial1;" >> "$output"
    echo "declare const getters: BigGetters;" >> "$output"
    echo "declare const deep: DeepBigObject;" >> "$output"
    echo "const _prop0 = partial.prop0;" >> "$output"
}

# Stress: TEMPLATE_LITERAL_EXPANSION_LIMIT (100,000)
generate_template_literal_file() {
    local variant_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Template literal type expansion stress test
// Tests Cartesian product explosion prevention

HEADER

    # Generate multiple union types for Cartesian product
    local max_variants=$((variant_count < 50 ? variant_count : 50))
    
    echo "type Colors =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'color$i';" >> "$output"
        else
            echo "    | 'color$i'" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    echo "type Sizes =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'size$i';" >> "$output"
        else
            echo "    | 'size$i'" >> "$output"
        fi
    done
    
    echo "" >> "$output"
    echo "type Variants =" >> "$output"
    for ((i=0; i<max_variants; i++)); do
        if [ $i -eq $((max_variants - 1)) ]; then
            echo "    | 'variant$i';" >> "$output"
        else
            echo "    | 'variant$i'" >> "$output"
        fi
    done
    
    # Template literal combining unions (Cartesian product)
    cat >> "$output" << 'EOF'

// Template literal Cartesian products
type ProductSmall = `${Colors}-${Sizes}`;
type ProductMedium = `${Colors}-${Sizes}-${Variants}`;

// String manipulation types
type Prefixed = `prefix_${Colors}`;
type Suffixed = `${Colors}_suffix`;
type Wrapped = `[${Colors}]`;

// Nested template
type NestedTemplate = `start_${`mid_${Colors}`}_end`;

EOF

    # Force evaluation
    echo "declare const product: ProductSmall;" >> "$output"
    echo "declare const prefixed: Prefixed;" >> "$output"
}

# Stress: MAX_SUBTYPE_DEPTH (100), coinductive cycle detection
generate_deep_subtype_file() {
    local depth="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Deep subtype checking stress test
// Tests recursive type comparison and cycle detection

// Self-referential types
interface TreeNode<T> {
    value: T;
    children: TreeNode<T>[];
}

interface MutualA<T> {
    data: T;
    ref: MutualB<T>;
}

interface MutualB<T> {
    info: T;
    back: MutualA<T>;
}

// Recursive JSON type
type Json = string | number | boolean | null | Json[] | { [key: string]: Json };

HEADER

    # Generate deep class hierarchy for variance checking
    echo "// Deep class hierarchy for subtype checking" >> "$output"
    echo "class Base0 { x0: string = ''; }" >> "$output"
    local max_depth=$((depth < 50 ? depth : 50))
    for ((i=1; i<max_depth; i++)); do
        local prev=$((i - 1))
        echo "class Base$i extends Base$prev { x$i: string = ''; }" >> "$output"
    done
    
    # Generate covariant/contravariant positions
    cat >> "$output" << 'EOF'

// Variance stress with function types
type CovariantContainer<T> = { get(): T };
type ContravariantContainer<T> = { set(x: T): void };
type InvariantContainer<T> = { get(): T; set(x: T): void };

// Bivariant method position
interface BivariantMethods<T> {
    method(x: T): T;
}

EOF

    # Deep nested function type
    local deepfn="string"
    local max_fn_depth=$((depth < 30 ? depth : 30))
    for ((i=0; i<max_fn_depth; i++)); do
        deepfn="(x: $deepfn) => void"
    done
    echo "" >> "$output"
    echo "type DeepFunction = $deepfn;" >> "$output"
    
    # Force subtype checks
    cat >> "$output" << 'EOF'

// Force subtype checks
declare const tree1: TreeNode<string>;
declare const tree2: TreeNode<string | number>;
const _check: TreeNode<string | number> = tree1;

declare const mutual: MutualA<string>;
declare function acceptMutual(x: MutualA<string | number>): void;
acceptMutual(mutual);

// JSON type checks
declare const json1: Json;
declare const json2: { nested: Json };
const _jsonCheck: Json = json2;

EOF
}

# Stress: Intersection normalization and property merging
generate_intersection_file() {
    local count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Intersection type stress test
// Tests intersection normalization and property merging

HEADER

    # Generate many interfaces to intersect
    for ((i=0; i<count; i++)); do
        echo "interface Part$i {" >> "$output"
        echo "    prop$i: string;" >> "$output"
        echo "    shared: number;" >> "$output"
        echo "    method$i(): number;" >> "$output"
        echo "}" >> "$output"
        echo "" >> "$output"
    done
    
    # Create large intersections
    local intersection="Part0"
    local max_intersect=$((count < 50 ? count : 50))
    for ((i=1; i<max_intersect; i++)); do
        intersection="$intersection & Part$i"
    done
    echo "type BigIntersection = $intersection;" >> "$output"
    
    # Function overload intersection
    cat >> "$output" << 'EOF'

// Function overload intersection
type OverloadIntersection = 
    ((x: string) => string) &
    ((x: number) => number) &
    ((x: boolean) => boolean);

// Generic intersection
type GenericIntersection<T, U> = T & U;

EOF

    # Force evaluation
    echo "" >> "$output"
    echo "declare const big: BigIntersection;" >> "$output"
    echo "const _prop0 = big.prop0;" >> "$output"
    echo "const _shared = big.shared;" >> "$output"
    local last=$((count - 1))
    if [ $last -lt 50 ]; then
        echo "const _propLast = big.prop$last;" >> "$output"
    fi
}

# Stress: Inference variable instantiation in conditional types
generate_infer_stress_file() {
    local count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Infer keyword stress test
// Tests inference variable resolution in conditional types

type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type UnwrapArray<T> = T extends (infer U)[] ? U : T;
type MyParameters<T> = T extends (...args: infer P) => any ? P : never;
type MyReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Multi-infer conditional
type FirstAndRest<T> = T extends [infer First, ...infer Rest] ? { first: First; rest: Rest } : never;

// Nested infer
type DeepUnwrap<T> = 
    T extends Promise<infer U> ? DeepUnwrap<U> :
    T extends (infer V)[] ? DeepUnwrap<V>[] :
    T;

// Infer in template literal
type ExtractPrefix<T> = T extends `${infer P}_${string}` ? P : never;

// Infer with constraints
type ExtractIfString<T> = T extends infer U extends string ? U : never;

HEADER

    # Generate functions with many parameters to test Parameters<T>
    local max_funcs=$((count < 30 ? count : 30))
    for ((i=0; i<max_funcs; i++)); do
        echo "declare function func$i(" >> "$output"
        for ((j=0; j<=i; j++)); do
            if [ $j -eq $i ]; then
                echo "    arg$j: string" >> "$output"
            else
                echo "    arg$j: string," >> "$output"
            fi
        done
        echo "): number;" >> "$output"
        echo "" >> "$output"
        echo "type Params$i = MyParameters<typeof func$i>;" >> "$output"
        echo "type Return$i = MyReturnType<typeof func$i>;" >> "$output"
        echo "" >> "$output"
    done
    
    # Force evaluation with complex nested inference
    cat >> "$output" << 'EOF'

// Complex nested inference
type ComplexInfer<T> = T extends { 
    data: infer D; 
    nested: { value: infer V }[] 
} ? { data: D; values: V[] } : never;

interface TestData {
    data: string;
    nested: { value: number }[];
}

type Inferred = ComplexInfer<TestData>;

EOF

    echo "declare const params: Params$((max_funcs - 1));" >> "$output"
    echo "declare const inferred: Inferred;" >> "$output"
}

# Stress: Control flow analysis with many branches
generate_cfa_stress_file() {
    local branch_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Control flow analysis stress test
// Tests type narrowing with many branches

type Status = 'pending' | 'active' | 'completed' | 'failed' | 'cancelled';

interface BaseEntity {
    id: string;
    status: Status;
}

HEADER

    # Generate discriminated union
    echo "type Entity =" >> "$output"
    for ((i=0; i<branch_count; i++)); do
        if [ $i -eq $((branch_count - 1)) ]; then
            echo "    | { kind: 'type$i'; data$i: string; common: number };" >> "$output"
        else
            echo "    | { kind: 'type$i'; data$i: string; common: number }" >> "$output"
        fi
    done
    
    # Generate exhaustive switch
    cat >> "$output" << 'EOF'

function processEntity(e: Entity): string {
    switch (e.kind) {
EOF

    for ((i=0; i<branch_count; i++)); do
        echo "        case 'type$i': return e.data$i;" >> "$output"
    done
    
    cat >> "$output" << 'EOF'
        default:
            throw new Error('unreachable');
    }
}

EOF

    # Generate many branch checks without relying on final-else narrowing.
    echo "function processWithIf(e: Entity): string {" >> "$output"
    for ((i=0; i<branch_count; i++)); do
        echo "    if (e.kind === 'type$i') {" >> "$output"
        echo "        return e.data$i;" >> "$output"
        echo "    }" >> "$output"
    done
    echo "    return processEntity(e);" >> "$output"
    echo "}" >> "$output"
    
    # Type guard functions
    echo "" >> "$output"
    for ((i=0; i<branch_count; i+=5)); do
        cat >> "$output" << EOF
function isType$i(e: Entity): e is Extract<Entity, { kind: 'type$i' }> {
    return e.kind === 'type$i';
}

EOF
    done
}

# =============================================================================
# O(N²) ALGORITHMIC PATTERN BENCHMARKS
# =============================================================================
# These generators create files that specifically stress the three known O(N²)
# algorithmic patterns in the solver that Salsa memoization alone cannot fix.
# See docs/todo/05_algorithmic_fixes.md for details.

# Stress: Best Common Type — O(N²) in infer.rs:1060
# N candidates × N subtype checks per candidate.
# Triggered when many return statements / array elements need a common type.
generate_bct_stress_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Best Common Type (BCT) O(N²) stress test
// Targets: infer.rs best_common_type() — N candidates × N subtype checks
//
// Each class in the hierarchy is a distinct type candidate. When the compiler
// infers the type of an array literal or multi-return function, it must find
// the "best common type" by checking every candidate against every other.

HEADER

    # Build a class hierarchy so types are related but distinct
    echo "class Base { base: string = ''; }" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "class Derived$i extends Base { prop$i: number = $i; }" >> "$output"
    done
    echo "" >> "$output"

    # 1. Array literal with N distinct derived types — triggers BCT
    echo "// Array literal: BCT must find common type among $count candidates" >> "$output"
    echo -n "const items = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "new Derived$i()" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    # 2. Function with N return statements — triggers BCT on return type
    echo "// Function with $count return branches — BCT on return type inference" >> "$output"
    echo "function pickOne(index: number) {" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "    if (index === $i) return new Derived$i();" >> "$output"
    done
    echo "    return new Base();" >> "$output"
    echo "}" >> "$output"
    echo "" >> "$output"

    # 3. Generic function called with N different argument types
    # This accumulates inference candidates that go through BCT
    echo "// Generic calls accumulating $count candidates" >> "$output"
    echo "function identity<T>(x: T): T { return x; }" >> "$output"
    echo -n "const mixed = [" >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n ", " >> "$output"; fi
        echo -n "identity(new Derived$i())" >> "$output"
    done
    echo "];" >> "$output"
    echo "" >> "$output"

    # 4. Conditional expression chains — each branch is a BCT candidate
    echo "// Ternary chain: $count candidates for common type" >> "$output"
    echo -n "declare const flag: number;" >> "$output"
    echo "" >> "$output"
    echo -n "const chosen = " >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n "flag === $i ? new Derived$i() : " >> "$output"
    done
    echo "new Base();" >> "$output"

    # Force type usage
    echo "" >> "$output"
    echo "const _base: Base = items[0];" >> "$output"
    echo "const _picked: Base = pickOne(0);" >> "$output"
    echo "const _chosen: Base = chosen;" >> "$output"
}

# Stress: Constraint Conflict Detection — O(N²) in infer.rs:135
# N² upper bound pairs + M×N lower×upper bound cross-checks.
# Triggered when a type parameter accumulates many bounds through usage.
generate_constraint_conflict_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Constraint Conflict Detection O(N²) stress test
// Targets: infer.rs detect_conflicts() — N² upper bound pairs + M×N lower×upper
//
// When a generic type parameter is used in many positions, the solver collects
// lower bounds (argument types) and upper bounds (extends constraints, parameter
// positions). Conflict detection checks all pairs for compatibility.

HEADER

    # Generate many interfaces that will become upper bounds
    for ((i=0; i<count; i++)); do
        echo "interface Constraint$i { key$i: string; shared: number; }" >> "$output"
    done
    echo "" >> "$output"

    # Function where T is constrained by many extends clauses via overloads/conditionals
    # Each call site adds bounds to T's constraint set
    echo "// Function with type parameter accumulating bounds from $count call sites" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "declare function constrain$i<T extends Constraint$i>(x: T): T;" >> "$output"
    done
    echo "" >> "$output"

    # Create objects satisfying various combinations of constraints
    echo "// Objects that satisfy multiple constraints" >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n "const obj$i = { shared: $i" >> "$output"
        # Each object satisfies constraints 0..i
        for ((j=0; j<=i && j<count; j++)); do
            echo -n ", key$j: 'val'" >> "$output"
        done
        echo " };" >> "$output"
    done
    echo "" >> "$output"

    # Call constrain functions — each call adds lower + upper bounds
    echo "// Each call adds lower bounds (arg type) and upper bounds (extends Constraint$i)" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "const res$i = constrain$i(obj$i);" >> "$output"
    done
    echo "" >> "$output"

    # Generic function that collects many bounds on a single type parameter
    echo "// Single type param T accumulating $count bounds" >> "$output"
    echo -n "function multiConstrained<T extends " >> "$output"
    for ((i=0; i<count; i++)); do
        if [ $i -gt 0 ]; then echo -n " & " >> "$output"; fi
        echo -n "Constraint$i" >> "$output"
    done
    echo ">(x: T): T { return x; }" >> "$output"
    echo "" >> "$output"

    # Build an object satisfying all constraints — forces full conflict check
    echo -n "const allConstraints = { shared: 0" >> "$output"
    for ((i=0; i<count; i++)); do
        echo -n ", key$i: 'val'" >> "$output"
    done
    echo " };" >> "$output"
    echo "const _result = multiConstrained(allConstraints);" >> "$output"
}

# Stress: Mapped Type Expansion with Complex Templates — O(N × template_size)
# in evaluate_rules/mapped.rs:157
# N properties × instantiate+evaluate per property, with non-trivial templates.
# The existing generate_mapped_type_file uses simple templates (T[K]).
# This version uses complex conditional templates that are expensive to evaluate.
generate_mapped_complex_template_file() {
    local key_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Mapped Type Complex Template Expansion O(N²) stress test
// Targets: evaluate_rules/mapped.rs — N properties × expensive template evaluation
//
// Unlike simple homomorphic mapped types ({ [K in keyof T]: T[K] }) where the
// template is trivial, these use conditional types and nested mapped types in
// the template position, making each property evaluation expensive.

// Utility types with non-trivial evaluation
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type Stringify<T> = { [K in keyof T]: T[K] extends number ? string : T[K] extends boolean ? 'true' | 'false' : T[K] extends string ? T[K] : string };
type Validate<T> = { [K in keyof T]: T[K] extends string ? { valid: true; value: T[K] } : T[K] extends number ? { valid: true; value: T[K] } : { valid: false; value: never } };
type Nullable<T> = { [K in keyof T]: T[K] | null | undefined };
type Promisify<T> = { [K in keyof T]: Promise<T[K]> };

// Complex conditional template: each property evaluation triggers conditional
// type distribution and nested type instantiation
type FormField<T> =
    T extends string ? { type: 'text'; value: T; validate: (v: string) => boolean }
  : T extends number ? { type: 'number'; value: T; validate: (v: number) => boolean }
  : T extends boolean ? { type: 'checkbox'; value: T; validate: (v: boolean) => boolean }
  : T extends (infer U)[] ? { type: 'list'; items: FormField<U>[]; validate: (v: U[]) => boolean }
  : T extends object ? { type: 'group'; fields: FormFields<T>; validate: (v: T) => boolean }
  : { type: 'unknown'; value: T };

type FormFields<T> = { [K in keyof T]: FormField<T[K]> };

HEADER

    # Generate a large interface with mixed property types
    echo "interface BigModel {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        local mod=$((i % 5))
        case $mod in
            0) echo "    field$i: string;" >> "$output" ;;
            1) echo "    field$i: number;" >> "$output" ;;
            2) echo "    field$i: boolean;" >> "$output" ;;
            3) echo "    field$i: string[];" >> "$output" ;;
            4) echo "    field$i: { nested: string; count: number };" >> "$output" ;;
        esac
    done
    echo "}" >> "$output"
    echo "" >> "$output"

    # Apply complex mapped types — each triggers per-property conditional evaluation
    echo "// Each mapped type application evaluates a conditional template for $key_count properties" >> "$output"
    echo "type BigForm = FormFields<BigModel>;" >> "$output"
    echo "type BigStringified = Stringify<BigModel>;" >> "$output"
    echo "type BigValidated = Validate<BigModel>;" >> "$output"
    echo "type BigNullable = Nullable<BigModel>;" >> "$output"
    echo "type BigPromises = Promisify<BigModel>;" >> "$output"
    echo "type BigDeepPartial = DeepPartial<BigModel>;" >> "$output"
    echo "" >> "$output"

    # Chained mapped types — composition multiplies the per-property cost
    echo "// Chained: each composition re-evaluates all $key_count properties" >> "$output"
    echo "type Chained1 = Nullable<Stringify<BigModel>>;" >> "$output"
    echo "type Chained2 = Validate<Nullable<BigModel>>;" >> "$output"
    echo "type Chained3 = FormFields<Nullable<BigModel>>;" >> "$output"
    echo "" >> "$output"

    # Force evaluation with declarations
    echo "declare const form: BigForm;" >> "$output"
    echo "declare const stringified: BigStringified;" >> "$output"
    echo "declare const validated: BigValidated;" >> "$output"
    echo "declare const chained: Chained3;" >> "$output"
    echo "" >> "$output"

    # Access properties to force full expansion
    echo "const _f0 = form.field0;" >> "$output"
    echo "const _s0 = stringified.field0;" >> "$output"
    echo "const _v0 = validated.field0;" >> "$output"
    local last=$((key_count - 1))
    echo "const _fLast = form.field$last;" >> "$output"
    echo "const _cLast = chained.field$last;" >> "$output"
}

# Stress: keyof + indexed-access chain depth.
# Many nested `keyof` and `T[K]` evaluations on a single deep object force
# repeated key-space materialization and indexed-access reduction.
generate_keyof_chain_file() {
    local depth="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// keyof + indexed-access chain stress test
// Targets: keyof materialization and indexed access reduction over deep shapes.

HEADER

    # Build a deeply nested interface where every level has a `next` field.
    echo "interface Level0 { v0: string; }" >> "$output"
    for ((i=1; i<depth; i++)); do
        local prev=$((i - 1))
        echo "interface Level$i { v$i: number; next: Level$prev; }" >> "$output"
    done
    echo "" >> "$output"

    # Force keyof evaluation at every level.
    for ((i=0; i<depth; i++)); do
        echo "type Keys$i = keyof Level$i;" >> "$output"
    done
    echo "" >> "$output"

    # Walk indexed access down the chain to its base case.
    echo -n "type DeepKey = Level$((depth - 1))" >> "$output"
    local n=$((depth - 1))
    while [ "$n" -gt 0 ]; do
        echo -n "[\"next\"]" >> "$output"
        n=$((n - 1))
    done
    echo ";" >> "$output"

    # Pick<> over every level — each application materializes a property key set.
    for ((i=0; i<depth; i++)); do
        echo "type Pick$i = Pick<Level$i, keyof Level$i>;" >> "$output"
    done
    echo "" >> "$output"

    echo "declare const deepest: DeepKey;" >> "$output"
    echo "const _v: number = (deepest as Level$((depth - 1))).v$((depth - 1));" >> "$output"
}

# Stress: function overload resolution.
# A single function name carries N overload signatures and is called with M
# call sites that have to be resolved against the full overload set.
generate_overload_resolution_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Function overload resolution stress test
// Targets: overload candidate ranking + applicability + signature instantiation.

HEADER

    # Declare an overloaded function with N signatures plus an implementation.
    for ((i=0; i<count; i++)); do
        echo "declare function pick(tag: 'tag$i', payload: { id$i: number }): { ok: true; tag: 'tag$i' };" >> "$output"
    done
    echo "declare function pick(tag: string, payload: unknown): { ok: false };" >> "$output"
    echo "" >> "$output"

    # N call sites covering most overload variants.
    for ((i=0; i<count; i++)); do
        echo "const r$i = pick('tag$i', { id$i: $i });" >> "$output"
    done
    echo "" >> "$output"

    # Generic wrapper that propagates the overload set through inference.
    cat >> "$output" << 'EOF'
function wrap<T extends 'tag0' | 'tag1' | 'tag2'>(
    tag: T,
    payload: { id0?: number; id1?: number; id2?: number }
) {
    return pick(tag as any, payload);
}
const _w = wrap('tag0', { id0: 0 });
EOF
}

# Stress: fresh object-literal assignment against a wide target type.
# Each assignment runs the per-property comparison and excess-property scan
# the checker performs on fresh literals, plus the typo-suggestion search.
# The generated fixture intentionally type-checks without errors so the
# benchmark exit code stays clean; the work being measured is the scan
# itself, not the error-reporting path.
generate_object_literal_assign_file() {
    local count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Wide object-literal assignment stress test
// Targets: fresh object-literal -> wide target type comparison and the
// suggestion-search the checker runs on every fresh literal assignment.

HEADER

    echo "interface Target {" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "    prop$i?: string;" >> "$output"
    done
    echo "}" >> "$output"
    echo "" >> "$output"

    # Each literal omits exactly one declared property, so the per-property
    # walk cannot short-circuit on an exact key-set match.
    for ((i=0; i<count; i++)); do
        echo -n "const obj$i: Target = { " >> "$output"
        local first=1
        for ((j=0; j<count; j++)); do
            if [ "$j" -eq "$i" ]; then continue; fi
            if [ "$first" -eq 1 ]; then
                first=0
            else
                echo -n ", " >> "$output"
            fi
            echo -n "prop$j: 'v$j'" >> "$output"
        done
        echo " };" >> "$output"
    done
    echo "" >> "$output"

    # Function-call form: same shape, exercised through parameter context.
    echo "declare function accept(t: Target): void;" >> "$output"
    for ((i=0; i<count; i++)); do
        echo "accept({ prop$i: 'v$i' });" >> "$output"
    done
}

# Stress: indexed access over a mapped reader table.
# Mirrors project-code patterns that repeatedly read through mapped helpers.
generate_indexed_access_hotspot_file() {
    local key_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Indexed-access hotspot benchmark.
// Mirrors project-code patterns that repeatedly read through mapped helpers.

HEADER

    echo "interface IndexedModel {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        echo "    prop$i: { value: number; tag: 'prop$i'; nested: { flag: boolean } };" >> "$output"
    done
    echo "}" >> "$output"
    cat >> "$output" << 'EOF'

type IndexedReaders<T> = { [K in keyof T]: (value: T[K]) => T[K] };
type IndexedValues<T> = { [K in keyof T]: T[K] }[keyof T];

declare const model: IndexedModel;
declare const readers: IndexedReaders<IndexedModel>;

function readIndexed<K extends keyof IndexedModel>(key: K): IndexedModel[K] {
    return readers[key](model[key]);
}

type AllIndexedValues = IndexedValues<IndexedModel>;
EOF

    for ((i=0; i<key_count; i++)); do
        cat >> "$output" << EOF
const indexedValue$i = readIndexed('prop$i').nested.flag ? readIndexed('prop$i').value : 0;
EOF
    done
}

generate_remapped_accessor_hotspot_file() {
    local key_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Remapped mapped-type accessor hotspot benchmark.
// Exercises template-literal key remapping plus indexed access values.

HEADER

    echo "interface AccessorModel {" >> "$output"
    for ((i=0; i<key_count; i++)); do
        echo "    prop$i: { id: number; label: 'prop$i' };" >> "$output"
    done
    echo "}" >> "$output"
    cat >> "$output" << 'EOF'

type AccessorPair<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K]
} & {
    [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void
};

declare const accessors: AccessorPair<AccessorModel>;
EOF

    for ((i=0; i<key_count; i++)); do
        cat >> "$output" << EOF
const accessorValue$i = accessors.getProp$i().id;
accessors.setProp$i({ id: accessorValue$i, label: 'prop$i' });
EOF
    done
}

generate_conditional_infer_hotspot_file() {
    local case_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Conditional infer hotspot benchmark.
// Exercises nested conditional extraction seen in utility-heavy projects.

type AsyncBox<T> = Promise<{ payload: T[]; meta: { created: string } }>;
type ExtractPayload<T> = T extends Promise<{ payload: (infer U)[] }> ? U : never;
type DeepUnwrap<T> =
    T extends Promise<infer U> ? DeepUnwrap<U> :
    T extends { payload: infer P } ? DeepUnwrap<P> :
    T extends (infer E)[] ? DeepUnwrap<E> :
    T;

HEADER

    for ((i=0; i<case_count; i++)); do
        cat >> "$output" << EOF
type ConditionalInput$i = AsyncBox<{ id: $i; nested: Promise<{ value: string; index: $i }> }>;
type ConditionalPayload$i = ExtractPayload<ConditionalInput$i>;
type ConditionalDeep$i = DeepUnwrap<ConditionalInput$i>;
declare const conditionalPayload$i: ConditionalPayload$i;
declare const conditionalDeep$i: ConditionalDeep$i;
const conditionalValue$i = conditionalPayload$i.id + conditionalDeep$i.id;

EOF
    done
}

generate_object_spread_hotspot_file() {
    local case_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Object spread hotspot benchmark.
// Exercises repeated object-literal spread inference and property merging.

interface SpreadBase {
    common: string;
    enabled: boolean;
}

HEADER

    for ((i=0; i<case_count; i++)); do
        cat >> "$output" << EOF
interface SpreadInput$i extends SpreadBase {
    value$i: number;
    nested$i: { readonly id: number; name: string };
}

declare const spreadInput$i: SpreadInput$i;
const spreadMerged$i = {
    ...spreadInput$i,
    extra$i: spreadInput$i.value$i,
    nested$i: { ...spreadInput$i.nested$i, name: spreadInput$i.common },
};
type SpreadResult$i = typeof spreadMerged$i;
const spreadCheck$i: SpreadResult$i = spreadMerged$i;

EOF
    done
}

generate_contextual_callback_hotspot_file() {
    local case_count="$1"
    local output="$2"

    cat > "$output" << 'HEADER'
// Contextual callback hotspot benchmark.
// Exercises mapped callback tables and indexed dispatch return preservation.

HEADER

    echo "interface EventMap {" >> "$output"
    for ((i=0; i<case_count; i++)); do
        echo "    event$i: { type: 'event$i'; value: number; payload: { id: number } };" >> "$output"
    done
    echo "}" >> "$output"
    cat >> "$output" << 'EOF'

type HandlerMap<T> = { [K in keyof T]: (event: T[K]) => T[K] };
declare const handlers: HandlerMap<EventMap>;

function dispatchEvent<K extends keyof EventMap>(kind: K, event: EventMap[K]): EventMap[K] {
    return handlers[kind](event);
}

EOF

    for ((i=0; i<case_count; i++)); do
        cat >> "$output" << EOF
const dispatched$i = dispatchEvent('event$i', { type: 'event$i', value: $i, payload: { id: $i } });
const dispatchedValue$i = dispatched$i.payload.id + dispatched$i.value;
EOF
    done
}
