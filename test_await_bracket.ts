// Test await in computed property in async context
async function test() {
  var x = { [await]: 123 };
}

// Test await in computed property in non-async context
function test2() {
  var x = { [await]: 123 };
}