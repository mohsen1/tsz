// Test case for fall-through narrowing
type Action = { type: "add" } | { type: "remove" } | { type: "update" };

function testFallThrough(action: Action) {
  switch (action.type) {
    case "add":
    case "remove":
      // action should be narrowed to { type: "add" } | { type: "remove" }
      const narrowed: "add" | "remove" = action.type;
      break;
    case "update":
      break;
  }
}

function testSimpleFallThrough(x: "a" | "b" | "c") {
  switch (x) {
    case "a":
    case "b":
      // x should be narrowed to "a" | "b"
      const narrowed2: "a" | "b" = x;
      break;
    case "c":
      break;
  }
}
