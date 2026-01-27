// Debug test for type alias resolution

type A = { x: number };
type B = { y: string };
type U = A | B;

// Test 1: Direct use of type alias
const u1: U = { x: 1 };
console.log(u1.x);  // Should work

// Test 2: Function parameter with type alias
function test(u: U) {
    console.log(u.x);  // Should work
}

// Test 3: Simple type alias
type UserId = number;
const id: UserId = 42;
console.log(id);  // Should work

// Test 4: Type alias reference in function
function getId(): UserId {
    return 42;
}

const id2 = getId();
console.log(id2);  // Should work

console.log("All tests completed");
