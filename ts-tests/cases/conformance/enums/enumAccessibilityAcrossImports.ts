// Test for enum accessibility across imports
// @ts-check

// @Filename: enumFile.ts
export enum Color {
    Red,
    Green,
    Blue
}

export enum Status {
    Pending = 0,
    Active = 1,
    Done = 2
}

export const enum Priority {
    Low = 1,
    Medium = 2,
    High = 3
}

// @Filename: testDefaultImport.ts
import ColorImport from './enumFile';
const x1: ColorImport = ColorImport.Red; // Should resolve
const x2 = ColorImport.Green; // Should resolve

// @Filename: testNamedImport.ts
import { Color, Status } from './enumFile';
const y1: Color = Color.Red; // Should resolve
const y2 = Status.Active; // Should resolve to 1
const y3 = Status.Done; // Should resolve to 2

// @Filename: testNamespaceImport.ts
import * as Enums from './enumFile';
const z1: Enums.Color = Enums.Color.Blue; // Should resolve
const z2 = Enums.Status.Pending; // Should resolve to 0
const z3 = Enums.Priority.High; // Should resolve to 3

// @Filename: testEnumMemberAccess.ts
import { Color } from './enumFile';
function getColorName(c: Color): string {
    switch (c) {
        case Color.Red: return "Red";
        case Color.Green: return "Green";
        case Color.Blue: return "Blue";
    }
}
const color1 = Color.Red;
const color2 = Color.Green;
const color3 = Color.Blue;

// @Filename: testEnumMergingAcrossFiles.ts
// @Filename: definitions.ts
export enum Direction {
    Up = 1,
    Down = 2
}

// @Filename: usage.ts
import { Direction } from './definitions';
const dir1 = Direction.Up; // Should resolve to 1
const dir2 = Direction.Down; // Should resolve to 2

// Test: Enum namespace merging with imports
// @Filename: enumWithNamespace.ts
export enum ErrorCode {
    NotFound = 404,
    ServerError = 500
}
export namespace ErrorCode {
    export function getMessage(code: ErrorCode): string {
        if (code === ErrorCode.NotFound) return "Not Found";
        if (code === ErrorCode.ServerError) return "Server Error";
        return "Unknown";
    }
}

// @Filename: useEnumWithNamespace.ts
import { ErrorCode } from './enumWithNamespace';
const err1 = ErrorCode.NotFound; // Should resolve to 404
const msg1 = ErrorCode.getMessage(ErrorCode.NotFound); // Should resolve to "Not Found"

// Test: Re-exported enums
// @Filename: reexport.ts
export { Color, Status } from './enumFile';
export * from './enumFile';

// @Filename: useReexport.ts
import { Color as ReexportedColor } from './reexport';
const c1: ReexportedColor = ReexportedColor.Red; // Should resolve

// Test: Type-only import of enum
// @Filename: typeOnlyImport.ts
import type { Color } from './enumFile';
// const t1: Color = Color.Red; // Error: Cannot use value
type ColorType = Color; // OK: type-only usage

// Test: Enum in object type
// @Filename: enumInType.ts
import { Status } from './enumFile';
interface User {
    status: Status;
}
const user: User = { status: Status.Active }; // Should resolve

// Test: Enum array
// @Filename: enumArray.ts
import { Color } from './enumFile';
const colors: Color[] = [Color.Red, Color.Green, Color.Blue]; // Should resolve all

// Test: Enum as computed property key
// @Filename: enumKey.ts
import { Color } from './enumFile';
const obj = {
    [Color.Red]: "red",
    [Color.Green]: "green",
    [Color.Blue]: "blue"
}; // Should resolve all
