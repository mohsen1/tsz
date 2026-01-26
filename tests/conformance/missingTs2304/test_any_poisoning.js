// Test case for "Any poisoning" effect
// When lib.d.ts is not loaded, references to console, Promise, Array, etc.
// should emit TS2304/TS2318/TS2583 errors, not silently return ANY
// These should emit TS2304/TS2318 when lib.d.ts is not loaded
console.log("hello"); // TS2304: Cannot find name 'console'
var p = new Promise(function (resolve) { return resolve(1); }); // TS2583/TS2304: Cannot find name 'Promise'
var arr = new Array(); // TS2304: Cannot find name 'Array'
// These should also emit errors
var obj = new Object(); // TS2304: Cannot find name 'Object'
var str = new String("hello"); // TS2304: Cannot find name 'String'
// Test that the "Any poisoning" doesn't suppress other errors
var x = console.log("test"); // Should get BOTH TS2304 for console AND TS2322 for type mismatch
