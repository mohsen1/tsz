// Test case for Rule #44: Module Augmentation Merging
// Interfaces with the same name in the same scope merge
// This crosses module boundaries via declare module "..."

// File a.ts
// export interface Window {
//   x: string;
// }

// File b.ts
// export interface Window {
//   y: number;
// }

// Expected result: Window has both x and y

// For single file, multiple interface declarations already work:
interface SingleFileWindow {
  x: string;
}

interface SingleFileWindow {
  y: number;
}

// This should work - SingleFileWindow has both x and y
const w1: SingleFileWindow = { x: "hello", y: 42 };

// Test with function parameter
function expectWindow(w: Window): void {}
expectWindow({ x: "test", y: 123 });
