
import wasmModule from "../pkg/wasm.js";

async function testFile() {
    const wasm = await wasmModule();
    
    const filePath = "ts-tests/cases/conformance/async/es2017/asyncArrowFunction/asyncArrowFunction6_es2017.ts";
    const result = wasm.check_files([filePath]);
    
    console.log("=== WASM Diagnostics for asyncArrowFunction6_es2017.ts ===");
    if (result && result.length > 0 && result[0].diagnostics) {
        result[0].diagnostics.forEach(d => {
            console.log(`TS${d.error_code}: ${d.message}`);
        });
    } else {
        console.log("No errors found");
    }
}

testFile().catch(console.error);

