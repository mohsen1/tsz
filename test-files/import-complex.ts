declare module "./mod" {
    export var a: number;
    export var b: number;
    export default 42;
}

import d from "./mod";
import * as ns from "./mod";
import { a, b } from "./mod";
import d2, { a as a2 } from "./mod";
