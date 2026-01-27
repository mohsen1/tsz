const u1 = { x: 1 };
console.log(u1.x);
function test(u) {
    console.log(u.x);
}
const id = 42;
console.log(id);
function getId() {
    return 42;
}
const id2 = getId();
console.log(id2);
console.log("All tests completed");
