// Test nested discriminant narrowing
// This test verifies that narrowing works for nested property paths like action.payload.kind

type Action =
  | { type: 'UPDATE', payload: { kind: 'user', data: { name: string } } }
  | { type: 'UPDATE', payload: { kind: 'product', data: { price: number } } }
  | { type: 'DELETE', id: number };

function reducer(action: Action) {
  // Test 1: Narrowing based on nested discriminant
  if (action.payload.kind === 'user') {
    // action.payload should be narrowed to { kind: 'user', data: { name: string } }
    const data = action.payload.data; // Should be { name: string }
    const name = data.name; // Should be string
    console.log(name);
  }

  // Test 2: Switch on nested discriminant
  switch (action.payload.kind) {
    case 'user':
      const userName = action.payload.data.name; // Should be string
      break;
    case 'product':
      const productPrice = action.payload.data.price; // Should be number
      break;
  }

  // Test 3: Multiple levels of nesting
  if (action.type === 'UPDATE') {
    if (action.payload.kind === 'user') {
      const userName = action.payload.data.name; // Should be string
      console.log(userName);
    }
  }
}

// Test 4: Function that takes narrowed payload
function handleUserPayload(payload: { kind: 'user', data: { name: string } }) {
  console.log(payload.data.name);
}

function processAction(action: Action) {
  if (action.payload.kind === 'user') {
    // action.payload should be narrowed correctly
    handleUserPayload(action.payload);
  }
}
