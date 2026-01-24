/// Minimal lib.d.ts for testing global type resolution
/// This provides basic built-in types and globals

// Primitive types
declare interface Object {}
declare interface Function {}
declare interface Array<T> {}
declare interface String {}
declare interface Number {}
declare interface Boolean {}

// ES2015+ types
declare interface Promise<T> {}
declare interface Map<K, V> {}
declare interface Set<T> {}
declare interface Symbol {}
declare interface Proxy<T> {}
declare interface Reflect {}

// DOM globals
declare var console: Console;
interface Console {
    log(...args: any[]): void;
    error(...args: any[]): void;
    warn(...args: any[]): void;
}

declare var window: Window;
interface Window {}

declare var document: Document;
interface Document {}

declare var globalThis: any;

// Node.js globals
declare var process: NodeProcess;
interface NodeProcess {}

declare var require: NodeRequire;
interface NodeRequire {
    (id: string): any;
}

// Math and JSON
declare var Math: Math;
interface Math {}

declare var JSON: JSON;
interface JSON {}

// Error types
declare interface Error {}
declare interface TypeError {}
declare interface ReferenceError {}
declare interface SyntaxError {}

// Date and RegExp
declare interface Date {}
declare interface RegExp {}

// Other intrinsics
declare var undefined: any;
declare var NaN: number;
declare var Infinity: number;
