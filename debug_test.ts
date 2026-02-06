let obj: { prop: string | number } = { prop: "ok" };
let key: "prop" = "prop";
if (typeof obj[key] === "string") {
    obj[key].toUpperCase();
}
