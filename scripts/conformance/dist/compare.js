/**
 * Diagnostic Comparison Utilities
 *
 * Compare diagnostic output between TypeScript compiler and tsz (WASM)
 * to measure conformance accuracy.
 */
/**
 * Compare diagnostics from tsc and wasm compilers
 */
export function compareDiagnostics(tscResult, wasmResult, includeDetails = false) {
    const tscCodes = tscResult.diagnostics.map(d => d.code);
    const wasmCodes = wasmResult.diagnostics.map(d => d.code);
    const tscCodeSet = new Set(tscCodes);
    const wasmCodeSet = new Set(wasmCodes);
    const missingInWasm = [...tscCodeSet].filter(c => !wasmCodeSet.has(c));
    const extraInWasm = [...wasmCodeSet].filter(c => !tscCodeSet.has(c));
    // Count matching codes (not necessarily same position)
    const matchingCodes = [...tscCodeSet].filter(c => wasmCodeSet.has(c)).length;
    // Check for exact match (same codes in same positions)
    const exactMatch = tscCodes.length === wasmCodes.length &&
        tscCodes.every((code, i) => wasmCodes[i] === code);
    const sameCount = tscResult.diagnostics.length === wasmResult.diagnostics.length;
    const result = {
        exactMatch,
        sameCount,
        tscCount: tscResult.diagnostics.length,
        wasmCount: wasmResult.diagnostics.length,
        missingInWasm,
        extraInWasm,
        matchingCodes,
    };
    if (includeDetails) {
        result.details = generateComparisonDetails(tscResult, wasmResult);
    }
    return result;
}
/**
 * Generate detailed comparison for each diagnostic
 */
function generateComparisonDetails(tscResult, wasmResult) {
    const details = [];
    const allCodes = new Set([
        ...tscResult.diagnostics.map(d => d.code),
        ...wasmResult.diagnostics.map(d => d.code),
    ]);
    for (const code of allCodes) {
        const tscDiag = tscResult.diagnostics.find(d => d.code === code);
        const wasmDiag = wasmResult.diagnostics.find(d => d.code === code);
        const detail = {
            code,
            inTsc: !!tscDiag,
            inWasm: !!wasmDiag,
        };
        if (tscDiag) {
            detail.tscMessage = tscDiag.message;
        }
        if (wasmDiag) {
            detail.wasmMessage = wasmDiag.message;
        }
        if (tscDiag && wasmDiag) {
            detail.messageMatch = tscDiag.message === wasmDiag.message;
        }
        details.push(detail);
    }
    return details;
}
/**
 * Compare diagnostics by code frequency
 * Useful when order doesn't matter but counts do
 */
export function compareByCodeFrequency(tscResult, wasmResult) {
    const tscCodeCounts = new Map();
    const wasmCodeCounts = new Map();
    for (const d of tscResult.diagnostics) {
        tscCodeCounts.set(d.code, (tscCodeCounts.get(d.code) || 0) + 1);
    }
    for (const d of wasmResult.diagnostics) {
        wasmCodeCounts.set(d.code, (wasmCodeCounts.get(d.code) || 0) + 1);
    }
    const allCodes = new Set([...tscCodeCounts.keys(), ...wasmCodeCounts.keys()]);
    const differences = [];
    let exactMatch = true;
    for (const code of allCodes) {
        const tscCount = tscCodeCounts.get(code) || 0;
        const wasmCount = wasmCodeCounts.get(code) || 0;
        if (tscCount !== wasmCount) {
            exactMatch = false;
            differences.push({ code, tscCount, wasmCount });
        }
    }
    return {
        exactMatch,
        tscCodeCounts,
        wasmCodeCounts,
        differences,
    };
}
/**
 * Format comparison result as a human-readable string
 */
export function formatComparison(comparison) {
    if (comparison.exactMatch) {
        return `EXACT MATCH (${comparison.tscCount} diagnostics)`;
    }
    const parts = [];
    if (comparison.sameCount) {
        parts.push(`same count (${comparison.tscCount})`);
    }
    else {
        parts.push(`tsc: ${comparison.tscCount}, wasm: ${comparison.wasmCount}`);
    }
    if (comparison.missingInWasm.length > 0) {
        const missing = comparison.missingInWasm.slice(0, 5).map(c => `TS${c}`).join(', ');
        const more = comparison.missingInWasm.length > 5
            ? ` +${comparison.missingInWasm.length - 5} more`
            : '';
        parts.push(`missing: ${missing}${more}`);
    }
    if (comparison.extraInWasm.length > 0) {
        const extra = comparison.extraInWasm.slice(0, 5).map(c => `TS${c}`).join(', ');
        const more = comparison.extraInWasm.length > 5
            ? ` +${comparison.extraInWasm.length - 5} more`
            : '';
        parts.push(`extra: ${extra}${more}`);
    }
    return parts.join(' | ');
}
/**
 * Format comparison result as JSON
 */
export function formatComparisonJson(comparison) {
    return JSON.stringify(comparison, null, 2);
}
/**
 * Calculate pass rate from comparison results
 */
export function calculatePassRate(comparisons) {
    const total = comparisons.length;
    const passed = comparisons.filter(c => c.exactMatch || c.sameCount).length;
    const exactMatch = comparisons.filter(c => c.exactMatch).length;
    return {
        total,
        passed,
        exactMatch,
        passRate: total > 0 ? (passed / total) * 100 : 0,
        exactMatchRate: total > 0 ? (exactMatch / total) * 100 : 0,
    };
}
/**
 * Group comparisons by error code for analysis
 */
export function groupByErrorCode(comparisons) {
    const missingCodes = new Map();
    const extraCodes = new Map();
    for (const { file, comparison } of comparisons) {
        for (const code of comparison.missingInWasm) {
            if (!missingCodes.has(code)) {
                missingCodes.set(code, []);
            }
            missingCodes.get(code).push(file);
        }
        for (const code of comparison.extraInWasm) {
            if (!extraCodes.has(code)) {
                extraCodes.set(code, []);
            }
            extraCodes.get(code).push(file);
        }
    }
    return { missingCodes, extraCodes };
}
/**
 * Get the most impactful error codes (by frequency)
 */
export function getMostImpactfulCodes(comparisons, limit = 10) {
    const { missingCodes, extraCodes } = groupByErrorCode(comparisons);
    const sortByCount = (codeMap) => {
        return [...codeMap.entries()]
            .map(([code, files]) => ({ code, count: files.length, files }))
            .sort((a, b) => b.count - a.count)
            .slice(0, limit);
    };
    return {
        missingCodes: sortByCount(missingCodes),
        extraCodes: sortByCount(extraCodes),
    };
}
//# sourceMappingURL=compare.js.map