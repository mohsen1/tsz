// @module: esnext
// @strict: true
// Test edge cases for import { x } vs let x

// ===== PART 1: Multiple imports from same module =====
// @Filename: types.ts
export const a = 1;
export const b = 2;
export const c = 3;

// @Filename: testMultipleImports.ts
import { a, b, c } from "./types";
let b = 10; // Error: Duplicate identifier 'b'

// ===== PART 2: Import alias vs local let =====
// @Filename: testImportAlias.ts
import { a as d } from "./types";
let d = 20; // Error: Duplicate identifier 'd'

// ===== PART 3: Re-export should not conflict with local =====
// @Filename: testReexport.ts
export { a } from "./types";
const a = 30; // Error: Duplicate identifier 'a'

// ===== PART 4: Namespace import vs local =====
// @Filename: testNamespaceImport.ts
import * as types from "./types";
const types = 40; // Error: Duplicate identifier 'types'

// ===== PART 5: Default import vs local =====
// @Filename: defaultExport.ts
export default 123;

// @Filename: testDefaultImport.ts
import defaultValue from "./defaultExport";
const defaultValue = 456; // Error: Duplicate identifier 'defaultValue'

// ===== PART 6: Type-only import should not conflict =====
// @Filename: types.ts (continued)
export type MyType = string;

// @Filename: testTypeOnlyImport.ts
import { type MyType } from "./types";
const MyType = 789; // Error: Duplicate identifier 'MyType'
