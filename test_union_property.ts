// Test union property access
type WithA = { a: string };
type WithA2 = { a: number };

function getProp(obj: WithA | WithA2) {
    return obj.a;
}

const obj1: WithA = { a: "hello" };
const obj2: WithA2 = { a: 42 };
const result1 = getProp(obj1); // Should be string
const result2 = getProp(obj2); // Should be number

// Test union variable
function testUnion(value: WithA | WithA2) {
    return value.a;
}


