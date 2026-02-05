// Test exhaustiveness checking for switch statements

type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "square"; side: number }
  | { kind: "triangle"; base: number; height: number };

function testExhaustive(shape: Shape): number {
  // This should be exhaustive - all cases covered
  switch (shape.kind) {
    case "circle":
      return shape.radius;
    case "square":
      return shape.side;
    case "triangle":
      return shape.base * shape.height / 2;
  }
}

function testNonExhaustive(shape: Shape): number {
  // ERROR: Missing "triangle" case
  switch (shape.kind) {
    case "circle":
      return shape.radius;
    case "square":
      return shape.side;
  }
}

function testNonExhaustiveNoReturn(shape: Shape): void {
  // ERROR: Missing cases even though return type is void
  switch (shape.kind) {
    case "circle":
      console.log(shape.radius);
      break;
    case "square":
      console.log(shape.side);
      break;
  }
}

function testWithDefault(shape: Shape): number {
  // Should be OK - has default case
  switch (shape.kind) {
    case "circle":
      return shape.radius;
    case "square":
      return shape.side;
    default:
      return 0;
  }
}

// Test with enum
enum Color {
  Red,
  Green,
  Blue
}

function testEnumExhaustive(color: Color): string {
  switch (color) {
    case Color.Red:
      return "red";
    case Color.Green:
      return "green";
    case Color.Blue:
      return "blue";
  }
}

function testEnumNonExhaustive(color: Color): string {
  // ERROR: Missing Color.Blue case
  switch (color) {
    case Color.Red:
      return "red";
    case Color.Green:
      return "green";
  }
}
