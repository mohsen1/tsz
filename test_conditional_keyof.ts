// Test conditional expression with keyof constraint

interface Shape {
  width: number;
  height: number;
}

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
  return obj[key];
}

const shape: Shape = { width: 100, height: 200 };

// This should work: "width" | "height" is assignable to keyof Shape
const cond = true;
const result = getProperty(shape, cond ? "width" : "height");
