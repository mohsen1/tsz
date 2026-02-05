// Test Array<T> method resolution after fix
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}

const result = map([1, 2, 3], x => x.toString());
// Expected: result has type string[]
// Expected: x has type number
