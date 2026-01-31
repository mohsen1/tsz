#!/usr/bin/env bash
#
# Benchmark: tsz vs tsgo (TypeScript 7 / typescript-go)
#
# Compares compilation performance across various file sizes and complexities.
# Requires: hyperfine (brew install hyperfine)
#
# Usage:
#   ./scripts/bench-vs-tsgo.sh           # Full benchmark suite
#   ./scripts/bench-vs-tsgo.sh --quick   # Quick smoke test (fewer runs, fewer files)
#   ./scripts/bench-vs-tsgo.sh --json    # Export results to JSON

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Compilers
TSZ="$PROJECT_ROOT/.target/release/tsz"
TSGO="${TSGO:-$(which tsgo 2>/dev/null || echo "")}"

# Parse arguments
QUICK_MODE=false
JSON_OUTPUT=false
for arg in "$@"; do
    case $arg in
        --quick) QUICK_MODE=true ;;
        --json) JSON_OUTPUT=true ;;
    esac
done

# Benchmark settings
if [ "$QUICK_MODE" = true ]; then
    WARMUP=1
    MIN_RUNS=3
    MAX_RUNS=5
    echo "Quick mode: fewer runs, subset of files"
else
    WARMUP=3
    MIN_RUNS=10
    MAX_RUNS=50
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_subheader() {
    echo
    echo -e "${CYAN}▶ $1${NC}"
    echo -e "${CYAN}─────────────────────────────────────────────────────────────────────────────${NC}"
}

file_info() {
    local file="$1"
    local lines=$(wc -l < "$file" 2>/dev/null | tr -d ' ')
    local bytes=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    local kb=$((bytes / 1024))
    echo "${lines} lines, ${kb}KB"
}

check_prerequisites() {
    print_header "Prerequisites Check"
    
    # Check hyperfine
    if ! command -v hyperfine &>/dev/null; then
        echo -e "${RED}✗ hyperfine not found${NC}"
        echo "  Install with: brew install hyperfine"
        exit 1
    fi
    echo -e "${GREEN}✓${NC} hyperfine $(hyperfine --version | head -1)"
    
    # Check jq (optional, for results table)
    if command -v jq &>/dev/null; then
        echo -e "${GREEN}✓${NC} jq $(jq --version)"
    else
        echo -e "${YELLOW}○${NC} jq not found (optional, install for results table)"
    fi
    
    # Check/build tsz
    if [ ! -x "$TSZ" ]; then
        echo -e "${YELLOW}Building tsz...${NC}"
        (cd "$PROJECT_ROOT" && cargo build --release --features cli)
    fi
    echo -e "${GREEN}✓${NC} tsz: $($TSZ --version 2>&1 | head -1)"
    
    # Check tsgo
    if [ -z "$TSGO" ] || [ ! -x "$TSGO" ]; then
        echo -e "${RED}✗ tsgo not found${NC}"
        echo "  Install with: npm install -g @typescript/native-preview"
        exit 1
    fi
    echo -e "${GREEN}✓${NC} tsgo: $($TSGO --version 2>&1 | head -1)"
}

RESULTS_CSV=""

run_benchmark() {
    local name="$1"
    local file="$2"
    local extra_args="${3:-}"
    
    local lines=$(wc -l < "$file" 2>/dev/null | tr -d ' ')
    local bytes=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    local kb=$((bytes / 1024))
    local info="${lines} lines, ${kb}KB"
    
    echo -e "${GREEN}$name${NC} ($info)"
    
    # Run benchmark and capture JSON output
    local json_file=$(mktemp)
    hyperfine \
        --warmup "$WARMUP" \
        --min-runs "$MIN_RUNS" \
        --max-runs "$MAX_RUNS" \
        --style full \
        --ignore-failure \
        --export-json "$json_file" \
        -n "tsz" "$TSZ --noEmit $extra_args $file 2>/dev/null" \
        -n "tsgo" "$TSGO --noEmit $extra_args $file 2>/dev/null"
    
    # Extract times and calculate throughput
    if [ -f "$json_file" ] && command -v jq &>/dev/null; then
        local tsz_mean=$(jq -r '.results[] | select(.command | contains("tsz")) | .mean' "$json_file" 2>/dev/null || echo "0")
        local tsgo_mean=$(jq -r '.results[] | select(.command | contains("tsgo")) | .mean' "$json_file" 2>/dev/null || echo "0")
        
        if [ -n "$tsz_mean" ] && [ -n "$tsgo_mean" ] && [ "$tsz_mean" != "0" ] && [ "$tsgo_mean" != "0" ]; then
            # Calculate throughput (lines/sec) and format times (2 decimal places)
            local tsz_lps=$(printf "%.0f" "$(echo "$lines / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_lps=$(printf "%.0f" "$(echo "$lines / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsz_ms=$(printf "%.2f" "$(echo "$tsz_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            local tsgo_ms=$(printf "%.2f" "$(echo "$tsgo_mean * 1000" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            
            # Determine winner and calculate speedup ratio
            local winner="tsgo"
            local ratio
            if (( $(echo "$tsz_mean < $tsgo_mean" | bc -l) )); then
                winner="tsz"
                ratio=$(printf "%.2f" "$(echo "$tsgo_mean / $tsz_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            else
                ratio=$(printf "%.2f" "$(echo "$tsz_mean / $tsgo_mean" | bc -l 2>/dev/null)" 2>/dev/null || echo "N/A")
            fi
            
            RESULTS_CSV="${RESULTS_CSV}${name},${lines},${kb},${tsz_ms},${tsgo_ms},${tsz_lps},${tsgo_lps},${winner},${ratio}\n"
        fi
    fi
    rm -f "$json_file"
}

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

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

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
            const result = await Promise.race([
                Promise.resolve(input),
                new Promise<never>((_, reject) => 
                    setTimeout(() => reject(new Error('timeout')), timeout)
                )
            ]);
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

generate_union_file() {
    local member_count="$1"
    local output="$2"
    
    cat > "$output" << 'HEADER'
// Union type stress test - discriminated unions with many members

HEADER

    # Generate union type
    echo "type Event =" >> "$output"
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
function handleEvent(event: Event): string {
    switch (event.type) {
HANDLER_START

    for ((i=0; i<member_count; i++)); do
        echo "        case 'event$i': return event.payload$i;" >> "$output"
    done
    
    cat >> "$output" << 'HANDLER_END'
    }
}

HANDLER_END

    # Generate some type narrowing tests
    for ((i=0; i<member_count; i+=10)); do
        cat >> "$output" << EOF
function isEvent$i(e: Event): e is Extract<Event, { type: 'event$i' }> {
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
    for ((i=0; i<max_chain; i++)); do
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
    }
}

EOF

    # Generate if-else chain with type guards
    echo "function processWithIf(e: Entity): string {" >> "$output"
    for ((i=0; i<branch_count; i++)); do
        if [ $i -eq 0 ]; then
            echo "    if (e.kind === 'type$i') {" >> "$output"
        elif [ $i -eq $((branch_count - 1)) ]; then
            echo "    } else {" >> "$output"
        else
            echo "    } else if (e.kind === 'type$i') {" >> "$output"
        fi
        echo "        return e.data$i;" >> "$output"
    done
    echo "    }" >> "$output"
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

main() {
    check_prerequisites
    
    # Create temp directory for synthetic files
    TEMP_DIR=$(mktemp -d)
    trap "rm -rf $TEMP_DIR" EXIT
    
    print_header "TypeScript Compiler Test Files"
    
    # ═══════════════════════════════════════════════════════════════════════════
    # EXTRA LARGE FILES (5000+ lines) - Stress tests
    # ═══════════════════════════════════════════════════════════════════════════
    print_subheader "Extra Large Files (5000+ lines) - Stress Tests"
    
    local xl_files
    if [ "$QUICK_MODE" = true ]; then
        xl_files=(
            "TypeScript/tests/cases/compiler/largeControlFlowGraph.ts"
            "TypeScript/tests/cases/compiler/manyConstExports.ts"
        )
    else
        xl_files=(
            "TypeScript/tests/cases/compiler/largeControlFlowGraph.ts"
            "TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts"
            "TypeScript/tests/cases/compiler/unionSubtypeReductionErrors.ts"
            "TypeScript/tests/cases/compiler/manyConstExports.ts"
            "TypeScript/tests/cases/compiler/binderBinaryExpressionStress.ts"
            "TypeScript/tests/cases/compiler/binderBinaryExpressionStressJs.ts"
        )
    fi
    
    for file in "${xl_files[@]}"; do
        local full_path="$PROJECT_ROOT/$file"
        if [ -f "$full_path" ]; then
            run_benchmark "$(basename "$file")" "$full_path"
            echo
        fi
    done
    
    # ═══════════════════════════════════════════════════════════════════════════
    # LARGE FILES (1000-5000 lines) - Real-world complexity
    # ═══════════════════════════════════════════════════════════════════════════
    print_subheader "Large Files (1000-5000 lines) - Real-world Complexity"
    
    local large_files
    if [ "$QUICK_MODE" = true ]; then
        large_files=(
            "TypeScript/tests/cases/compiler/enumLiteralsSubtypeReduction.ts"
        )
    else
        large_files=(
            "TypeScript/tests/cases/compiler/enumLiteralsSubtypeReduction.ts"
            "TypeScript/tests/cases/compiler/binaryArithmeticControlFlowGraphNotTooLarge.ts"
            "TypeScript/tests/cases/compiler/privacyFunctionReturnTypeDeclFile.ts"
            "TypeScript/tests/cases/compiler/privacyAccessorDeclFile.ts"
            "TypeScript/tests/cases/compiler/resolvingClassDeclarationWhenInBaseTypeResolution.ts"
        )
    fi
    
    for file in "${large_files[@]}"; do
        local full_path="$PROJECT_ROOT/$file"
        if [ -f "$full_path" ]; then
            run_benchmark "$(basename "$file")" "$full_path"
            echo
        fi
    done
    
    # Skip medium/small files in quick mode
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Skipping medium/small files in quick mode"
    else
        # ═══════════════════════════════════════════════════════════════════════════
        # MEDIUM FILES (200-1000 lines) - Typical modules
        # ═══════════════════════════════════════════════════════════════════════════
        print_subheader "Medium Files (200-1000 lines) - Typical Modules"
        
        local medium_files=(
            "TypeScript/tests/cases/compiler/privacyFunctionParameterDeclFile.ts"
            "TypeScript/tests/cases/compiler/complexNarrowingWithAny.ts"
            "TypeScript/tests/cases/compiler/privacyGloFunc.ts"
            "TypeScript/tests/cases/compiler/privacyTypeParameterOfFunctionDeclFile.ts"
            "TypeScript/tests/cases/compiler/privacyVarDeclFile.ts"
            "TypeScript/tests/cases/compiler/deeplyDependentLargeArrayMutation2.ts"
        )
    
        for file in "${medium_files[@]}"; do
            local full_path="$PROJECT_ROOT/$file"
            if [ -f "$full_path" ]; then
                run_benchmark "$(basename "$file")" "$full_path"
                echo
            fi
        done
        
        # ═══════════════════════════════════════════════════════════════════════════
        # SMALL FILES (50-200 lines) - Quick iteration
        # ═══════════════════════════════════════════════════════════════════════════
        print_subheader "Small Files (50-200 lines) - Startup Overhead Test"
        
        local small_files=(
            "TypeScript/tests/cases/compiler/typedArrays.ts"
            "TypeScript/tests/cases/compiler/bluebirdStaticThis.ts"
            "TypeScript/tests/cases/compiler/privacyVar.ts"
        )
        
        for file in "${small_files[@]}"; do
            local full_path="$PROJECT_ROOT/$file"
            if [ -f "$full_path" ]; then
                run_benchmark "$(basename "$file")" "$full_path"
                echo
            fi
        done
    fi  # End of medium/small files skip
    
    print_header "Synthetic Benchmarks - Scaling Test"
    
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced synthetic tests"
        
        # Just one of each type in quick mode
        local file="$TEMP_DIR/synthetic_100_classes.ts"
        generate_synthetic_file 100 "$file"
        run_benchmark "100 classes" "$file"
        echo
        
        file="$TEMP_DIR/complex_50_funcs.ts"
        generate_complex_file 50 "$file"
        run_benchmark "50 generic functions" "$file"
        echo
    else
        # Generate synthetic files of increasing size
        print_subheader "Class-heavy files (interfaces + classes)"
        
        for count in 10 50 100 200; do
            local file="$TEMP_DIR/synthetic_${count}_classes.ts"
            generate_synthetic_file "$count" "$file"
            run_benchmark "${count} classes" "$file"
            echo
        done
        
        print_subheader "Generic-heavy files (async + conditional types)"
        
        for count in 20 50 100 200; do
            local file="$TEMP_DIR/complex_${count}_funcs.ts"
            generate_complex_file "$count" "$file"
            run_benchmark "${count} generic functions" "$file"
            echo
        done
        
        print_subheader "Union type stress test"
        
        for count in 50 100 200; do
            local file="$TEMP_DIR/union_${count}.ts"
            generate_union_file "$count" "$file"
            run_benchmark "${count} union members" "$file"
            echo
        done
    fi
    
    # ═══════════════════════════════════════════════════════════════════════════
    # SOLVER STRESS TESTS - Type system limit testing
    # ═══════════════════════════════════════════════════════════════════════════
    print_header "Solver Stress Tests - Type System Limits"
    
    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced solver stress tests"
        
        # One test per category in quick mode
        local file="$TEMP_DIR/recursive_generic_25.ts"
        generate_recursive_generic_file 25 "$file"
        run_benchmark "Recursive generic depth=25" "$file"
        echo
        
        file="$TEMP_DIR/conditional_dist_50.ts"
        generate_conditional_distribution_file 50 "$file"
        run_benchmark "Conditional dist N=50" "$file"
        echo
        
        file="$TEMP_DIR/mapped_100.ts"
        generate_mapped_type_file 100 "$file"
        run_benchmark "Mapped type keys=100" "$file"
        echo
    else
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Recursive generic instantiation (MAX_INSTANTIATION_DEPTH=50)"
        
        for depth in 20 35 45; do
            local file="$TEMP_DIR/recursive_generic_${depth}.ts"
            generate_recursive_generic_file "$depth" "$file"
            run_benchmark "Recursive generic depth=$depth" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Conditional type distribution (MAX_DISTRIBUTION_SIZE=100)"
        
        for count in 50 80 95; do
            local file="$TEMP_DIR/conditional_dist_${count}.ts"
            generate_conditional_distribution_file "$count" "$file"
            run_benchmark "Conditional dist N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Mapped type expansion (MAX_MAPPED_KEYS=500)"
        
        for count in 100 300 450; do
            local file="$TEMP_DIR/mapped_${count}.ts"
            generate_mapped_type_file "$count" "$file"
            run_benchmark "Mapped type keys=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Template literal types (TEMPLATE_LITERAL_EXPANSION_LIMIT)"
        
        for count in 20 35 45; do
            local file="$TEMP_DIR/template_${count}.ts"
            generate_template_literal_file "$count" "$file"
            run_benchmark "Template literal N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Deep subtype checking (MAX_SUBTYPE_DEPTH=100)"
        
        for depth in 30 60 90; do
            local file="$TEMP_DIR/deep_subtype_${depth}.ts"
            generate_deep_subtype_file "$depth" "$file"
            run_benchmark "Deep subtype depth=$depth" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Intersection types (property merging)"
        
        for count in 20 35 45; do
            local file="$TEMP_DIR/intersection_${count}.ts"
            generate_intersection_file "$count" "$file"
            run_benchmark "Intersection N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Infer keyword stress (type inference)"
        
        for count in 15 25 30; do
            local file="$TEMP_DIR/infer_${count}.ts"
            generate_infer_stress_file "$count" "$file"
            run_benchmark "Infer stress N=$count" "$file"
            echo
        done
        
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Control flow analysis (CFA with many branches)"
        
        for count in 50 100 150; do
            local file="$TEMP_DIR/cfa_${count}.ts"
            generate_cfa_stress_file "$count" "$file"
            run_benchmark "CFA branches=$count" "$file"
            echo
        done
    fi

    # ═══════════════════════════════════════════════════════════════════════════
    # O(N²) ALGORITHMIC PATTERN TESTS
    # ═══════════════════════════════════════════════════════════════════════════
    # These benchmarks target three specific O(N²) patterns in the solver that
    # Salsa memoization alone cannot fix. They serve as regression/progress
    # tracking for the algorithmic fixes described in docs/todo/05_algorithmic_fixes.md
    #
    # Pattern 1: Best Common Type (BCT) — infer.rs:1060
    #   N candidates × N subtype checks per candidate
    # Pattern 2: Constraint Conflict Detection — infer.rs:135
    #   N² upper bound pairs + M×N lower×upper cross-checks
    # Pattern 3: Mapped Type Complex Templates — evaluate_rules/mapped.rs:157
    #   N properties × expensive per-property template evaluation

    print_header "O(N²) Algorithmic Pattern Tests"

    if [ "$QUICK_MODE" = true ]; then
        print_subheader "Quick mode: reduced O(N²) pattern tests"

        local file="$TEMP_DIR/bct_50.ts"
        generate_bct_stress_file 50 "$file"
        run_benchmark "BCT candidates=50" "$file"
        echo

        file="$TEMP_DIR/constraint_conflict_30.ts"
        generate_constraint_conflict_file 30 "$file"
        run_benchmark "Constraint conflicts N=30" "$file"
        echo

        file="$TEMP_DIR/mapped_complex_50.ts"
        generate_mapped_complex_template_file 50 "$file"
        run_benchmark "Mapped complex template keys=50" "$file"
        echo
    else
        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Best Common Type — O(N²) candidate checking"

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/bct_${count}.ts"
            generate_bct_stress_file "$count" "$file"
            run_benchmark "BCT candidates=$count" "$file"
            echo
        done

        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Constraint Conflict Detection — O(N²) bound pairs"

        for count in 20 50 100 200; do
            local file="$TEMP_DIR/constraint_conflict_${count}.ts"
            generate_constraint_conflict_file "$count" "$file"
            run_benchmark "Constraint conflicts N=$count" "$file"
            echo
        done

        # ─────────────────────────────────────────────────────────────────────────
        print_subheader "Mapped Type Complex Templates — O(N × template_cost)"

        for count in 25 50 100 200; do
            local file="$TEMP_DIR/mapped_complex_${count}.ts"
            generate_mapped_complex_template_file "$count" "$file"
            run_benchmark "Mapped complex template keys=$count" "$file"
            echo
        done
    fi

    print_header "Results Summary"
    
    if command -v jq &>/dev/null && [ -n "$RESULTS_CSV" ]; then
        echo
        # Table header
        printf "${BOLD}%-45s %7s %6s %10s %10s %8s %8s${NC}\n" \
            "Test" "Lines" "KB" "tsz(ms)" "tsgo(ms)" "Winner" "Factor"
        printf "${CYAN}%s${NC}\n" "─────────────────────────────────────────────────────────────────────────────────────────────────"
        
        # Table rows
        echo -e "$RESULTS_CSV" | while IFS=',' read -r name lines kb tsz_ms tsgo_ms tsz_lps tsgo_lps winner ratio; do
            [ -z "$name" ] && continue
            
            # Truncate long test names
            local display_name="$name"
            if [ ${#name} -gt 44 ]; then
                display_name="${name:0:41}..."
            fi
            
            # Color the winner and show factor
            if [ "$winner" = "tsz" ]; then
                printf "%-45s %7s %6s %10s %10s ${GREEN}%8s${NC} ${GREEN}%7sx${NC}\n" \
                    "$display_name" "$lines" "$kb" "$tsz_ms" "$tsgo_ms" "$winner" "$ratio"
            else
                printf "%-45s %7s %6s %10s %10s ${YELLOW}%8s${NC} ${YELLOW}%7sx${NC}\n" \
                    "$display_name" "$lines" "$kb" "$tsz_ms" "$tsgo_ms" "$winner" "$ratio"
            fi
        done
        
        # Summary line
        printf "${CYAN}%s${NC}\n" "─────────────────────────────────────────────────────────────────────────────────────────────────"
        
        # Count wins
        local tsz_wins=$(echo -e "$RESULTS_CSV" | grep -c ",tsz," || echo "0")
        local tsgo_wins=$(echo -e "$RESULTS_CSV" | grep -c ",tsgo," || echo "0")
        echo
        echo -e "${BOLD}Score:${NC} ${GREEN}tsz ${tsz_wins}${NC} vs ${YELLOW}tsgo ${tsgo_wins}${NC}"
        echo
    fi
}

main "$@"
