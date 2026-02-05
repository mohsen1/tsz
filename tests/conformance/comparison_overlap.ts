// Test cases for TS2367: Comparison overlap detection
// https://github.com/microsoft/TypeScript/blob/main/src/compiler/diagnosticMessages.json#L2367

// Should emit TS2367: number & string have no overlap
if (1 === "one") {
    console.log("never");
}

// Should emit TS2367: boolean & number have no overlap
if (true === 1) {
    console.log("never");
}

// Should emit TS2367: string & boolean have no overlap
if ("hello" !== false) {
    console.log("never");
}

// Should NOT emit TS2367: both number, overlap possible
if (1 === 2) {
    console.log("possible");
}

// Should NOT emit TS2367: both string, overlap possible
if ("a" === "b") {
    console.log("possible");
}

// Should emit TS2367: object types with incompatible property types
interface A { x: string }
interface B { x: number }
declare const a: A;
declare const b: B;
if (a === b) {
    console.log("never");
}

// Should NOT emit TS2367: any type suppresses error
declare const anything: any;
if (1 === anything) {
    console.log("possible");
}

// Should NOT emit TS2367: unknown type suppresses error
declare const unk: unknown;
if (1 === unk) {
    console.log("possible");
}

// Literal types: should emit TS2367 for different literals
if ("a" === "b") {
    console.log("possible - same type");
}

// Loose equality: should still emit TS2367
if (1 == "one") {
    console.log("never");
}

// Negation: should emit TS2367 with "always return 'true'"
if (1 !== "one") {
    console.log("always true");
}
