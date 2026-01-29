class Base {
    baseMethod() { return 1; }
}

class Derived extends Base {
    // Case 1: Arrow function in field initializer (Should be allowed)
    arrowField = () => super.baseMethod();

    // Case 2: Nested arrow function in method (Should be allowed)
    method() {
        const nested = () => super.baseMethod();
        return nested();
    }

    // Case 3: Arrow function in constructor (Should be allowed)
    constructor() {
        super();
        const inCtor = () => super.baseMethod();
    }

    // Case 4: Super in static arrow (Should be allowed if accessing static super)
    static staticArrow = () => super.toString();
}
