// Test file for strict null checks implementation
// These tests verify the behavior of null/undefined assignability

// Non-strict mode (strictNullChecks: false)
// null and undefined should be assignable to any type

// Strict mode (strictNullChecks: true)
// null is only assignable to: null, any, unknown, or unions containing null
// undefined is only assignable to: undefined, any, unknown, void, or unions containing undefined

// Test 1: null to string (should fail in strict mode)
let x1: string = null;

// Test 2: null to string | null (should pass in both modes)
let x2: string | null = null;

// Test 3: undefined to string (should fail in strict mode)
let x3: string = undefined;

// Test 4: undefined to string | undefined (should pass in both modes)
let x4: string | undefined = undefined;

// Test 5: undefined to void (should pass in both modes)
let x5: void = undefined;

// Test 6: null to any (should pass in both modes)
let x6: any = null;

// Test 7: null to unknown (should pass in both modes)
let x7: unknown = null;

// Test 8: undefined to any (should pass in both modes)
let x8: any = undefined;

// Test 9: undefined to unknown (should pass in both modes)
let x9: unknown = undefined;
