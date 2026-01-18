```javascript
/**
 * Helper function to check for 'use strict' compliance.
 * @param {string} code - The source code string to check.
 * @param {boolean} isModule - Whether the code is treated as a Module (automatically strict).
 * @returns {boolean} - True if the code enforces strict mode.
 */
function checkStrictMode(code, isModule = false) {
    // Modules are implicitly strict, regardless of the directive string.
    if (isModule) return true;

    // Trim whitespace to handle indentation, but preserve internal structure.
    const body = code.trim();

    // 1. Simple Global Check: If the very first statement is 'use strict'.
    // Note: Regex handles 'use strict', "use strict", and whitespace variations.
    const strictDirectiveRegex = /^(?:["']use strict["'];?\s*)+/;

    // We assume for this utility that 'code' represents a function body or script body.
    // If it starts with the directive, strict mode is active.
    if (strictDirectiveRegex.test(body)) {
        return true;
    }

    return false;
}

/**
 * Test Suite Runner
 */
function runTests() {
    let passed = 0;
    let total = 0;

    const test = (description, code, isModule, expected) => {
        total++;
        const result = checkStrictMode(code, isModule);
        const status = result === expected ? 'PASS' : 'FAIL';
        
        if (status === 'PASS') passed++;
        else {
            console.error(`[FAIL] ${description}`);
            console.error(`  Expected: ${expected}, Got: ${result}`);
            console.error(`  Code: "${code}"`);
        }
    };

    console.log("--- Running 'use strict' Logic Tests ---");

    // --- POSITIVE CASES (Should be Strict) ---

    test(
        "Standard Double Quote",
        `"use strict"; var x = 10;`,
        false,
        true
    );

    test(
        "Standard Single Quote",
        `'use strict'; var x = 10;`,
        false,
        true
    );

    test(
        "With Semicolon",
        `"use strict";`,
        false,
        true
    );

    test(
        "Without Semicolon (valid expression statement)",
        `"use strict" var x = 10;`, // Note: Intentionally missing semicolon in test string logic, but typically parsed as directive if first.
        false,
        true // Logic: The regex checks for the existence of the directive string first.
    );

    test(
        "Module Scope (Implicit Strict)",
        `var x = 10;`, // No directive needed
        true,
        true
    );

    // --- NEGATIVE CASES (Should NOT be Strict) ---

    test(
        "False Negative: Preceding Comment",
        `// My comment\n "use strict";`,
        false,
        false // Directives must be the FIRST statements. Comments separate them.
    );

    test(
        "False Negative: Preceding Statement",
        `var x = 0; "use strict";`,
        false,
        false
    );

    test(
        "False Negative: String with Extra Text (Not a directive)",
        `var x = "This is not use strict";`,
        false,
        false
    );

    test(
        "False Negative: Directive in Middle of Function",
        `function test() { var a = 1; "use strict"; var b = 2; }`,
        false,
        false
    );

    test(
        "False Negative: Backticks",
        "`use strict`; var x = 10;",
        false,
        false // Template literals are NOT directives.
    );

    console.log(`\nTests Passed: ${passed}/${total}`);
}

// Execute tests
runTests();
```
