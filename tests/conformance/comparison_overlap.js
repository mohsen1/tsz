if (1 === "one") {
    console.log("never");
}
if (true === 1) {
    console.log("never");
}
if ("hello" !== false) {
    console.log("never");
}
if (1 === 2) {
    console.log("possible");
}
if ("a" === "b") {
    console.log("possible");
}
if (a === b) {
    console.log("never");
}
if (1 === anything) {
    console.log("possible");
}
if (1 === unk) {
    console.log("possible");
}
if ("a" === "b") {
    console.log("possible - same type");
}
if (1 == "one") {
    console.log("never");
}
if (1 !== "one") {
    console.log("always true");
}
