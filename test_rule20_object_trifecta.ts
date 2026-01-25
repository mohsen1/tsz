// Test Rule #20: Object vs object vs {} Trifecta
//
// This test verifies that:
// 1. Object (Global Interface): Accepts everything except null/undefined (including primitives)
// 2. {} (Empty Object Type): Accepts everything except null/undefined
// 3. object (lowercase): Accepts ONLY non-primitives

// Test 1: Object (uppercase global interface) - should accept primitives
let obj1: Object = 42; // ✅ should accept number
let obj2: Object = "hello"; // ✅ should accept string
let obj3: Object = true; // ✅ should accept boolean
let obj4: Object = 123n; // ✅ should accept bigint
let obj5: Object = Symbol(); // ✅ should accept symbol
let obj6: Object = {}; // ✅ should accept empty object
let obj7: Object = []; // ✅ should accept array
let obj8: Object = () => {}; // ✅ should accept function

// Test 2: {} (empty object type) - should accept primitives
let empty1: {} = 42; // ✅ should accept number
let empty2: {} = "hello"; // ✅ should accept string
let empty3: {} = true; // ✅ should accept boolean
let empty4: {} = 123n; // ✅ should accept bigint
let empty5: {} = Symbol(); // ✅ should accept symbol
let empty6: {} = {}; // ✅ should accept empty object
let empty7: {} = []; // ✅ should accept array
let empty8: {} = () => {}; // ✅ should accept function

// Test 3: object (lowercase) - should NOT accept primitives
let lower1: object = 42; // ❌ should NOT accept number
let lower2: object = "hello"; // ❌ should NOT accept string
let lower3: object = true; // ❌ should NOT accept boolean
let lower4: object = 123n; // ❌ should NOT accept bigint
let lower5: object = Symbol(); // ❌ should NOT accept symbol
let lower6: object = {}; // ✅ should accept empty object
let lower7: object = []; // ✅ should accept array
let lower8: object = () => {}; // ✅ should accept function

// Test 4: All three should reject null/undefined/void
let objNull: Object = null; // ❌ should reject null
let objUndef: Object = undefined; // ❌ should reject undefined
let emptyNull: {} = null; // ❌ should reject null
let emptyUndef: {} = undefined; // ❌ should reject undefined
let lowerNull: object = null; // ❌ should reject null
let lowerUndef: object = undefined; // ❌ should reject undefined
