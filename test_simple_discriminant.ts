type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "square"; side: number };

function test(shape: Shape) {
  if (shape.kind !== "circle") {
    // shape should be { kind: "square"; side: number }
    const s: number = shape.side;
  }
}
