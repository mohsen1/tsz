type Shape = { kind: "circle" } | { kind: "square" };

function testVoid(shape: Shape): void {
  switch (shape.kind) {
    case "circle":
      console.log("circle");
      break;
  }
  // Missing "square" case, but return type is void so no error expected
}

function testNumber(shape: Shape): number {
  switch (shape.kind) {
    case "circle":
      return 1;
  }
  // Missing "square" case, return type is number so error expected
}
