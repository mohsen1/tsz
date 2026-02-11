// In TypeScript, {} means "any non-nullish value", not "empty object"

let a: {} = 5;           // Should work - number is non-nullish
let b: {} = "hello";     // Should work - string is non-nullish
let c: {} = true;        // Should work - boolean is non-nullish
let d: {} = {};          // Should work - object is non-nullish
let e: {} = [];          // Should work - array is non-nullish
let f: {} = null;        // Should FAIL - null is nullish
let g: {} = undefined;   // Should FAIL - undefined is nullish
