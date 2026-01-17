// Test various patterns that should emit TS1109
async function test() {
  // Incomplete await
  await;

  // Incomplete yield
  function* gen() {
    yield;
  }

  // New without constructor
  new ();

  // Return without value in specific contexts
  return;
}