// Test file to verify basic globals resolution
// This file should NOT produce TS2304 errors for the built-in globals

// Test console
console.log("Hello");

// Test Array
const arr: Array<number> = [1, 2, 3];

// Test Object
const obj: Object = new Object();

// Test Promise
const prom: Promise<number> = Promise.resolve(42);

// Test Error
const err: Error = new Error("test");

// Test Map
const map: Map<string, number> = new Map();

// Test Set
const set: Set<number> = new Set();

// Test String
const str: String = new String("test");

// Test Number
const num: Number = new Number(42);

// Test Boolean
const bool: Boolean = new Boolean(true);

// Test Date
const date: Date = new Date();

// Test Math
const mathResult: number = Math.abs(-5);

// Test JSON
const jsonResult: unknown = JSON.parse("{}");

// Test Function
const func: Function = () => {};
