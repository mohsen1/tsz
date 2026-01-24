// Test TS2307 for import equals declarations (require)

// Test 1: Import equals with require - missing module
import MissingRequire = require('./missing-require');

// Test 2: Import equals with namespace - missing
import MissingNs = MissingNamespace.Inner;

// Test 3: Import equals from external package - missing
import MissingPkg = require('missing-external-package');

// Test 4: Type-only import equals - missing
import type MissingType = require('./missing-type-require');

// Using the imports to trigger type checking
function testUsage() {
    const mr = new MissingRequire();
    const mn = MissingNs.value;
    const mp = MissingPkg.default;
    const mt: MissingType = null;
}
