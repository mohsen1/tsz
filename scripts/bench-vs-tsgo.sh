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
