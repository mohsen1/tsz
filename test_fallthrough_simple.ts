// Test case for fall-through narrowing with literals only
function testSimpleFallThrough(x: "a" | "b" | "c") {
  switch (x) {
    case "a":
    case "b":
      // x should be narrowed to "a" | "b"
      const narrowed: "a" | "b" = x;
      break;
    case "c":
      break;
  }
}
