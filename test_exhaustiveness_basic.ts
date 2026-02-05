// Test basic exhaustiveness checking for discriminant unions

type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "square"; side: number };

function testExhaustive(shape: Shape): number {
  // All cases covered - should be OK
  switch (shape.kind) {
    case "circle":
      return shape.radius;
    case "square":
      return shape.side;
  }
}

function testNonExhaustive(shape: Shape): number {
  // Missing "square" case - should error
  switch (shape.kind) {
    case "circle":
      return shape.radius;
  }
}

function testWithDefault(shape: Shape): number {
  // Has default - should be OK
  switch (shape.kind) {
    case "circle":
      return shape.radius;
    default:
      return 0;
  }
}

function testVoidNonExhaustive(shape: Shape): void {
  // Missing cases but return type is void - should NOT error from our check
  switch (shape.kind) {
    case "circle":
      console.log(shape.radius);
      break;
  }
}
