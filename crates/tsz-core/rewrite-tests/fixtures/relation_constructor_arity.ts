abstract class Base {}

class Derived extends Base {
    constructor(seed: number) {
        super();
    }
}

const concrete: typeof Base = Derived;
const Wrapped = Derived;
const wrapped: typeof Base = Wrapped;

abstract class GenericBase<T> {}

class GenericDerived<T> extends GenericBase<T> {
    constructor(seed: T) {
        super();
    }
}

const generic: typeof GenericBase = GenericDerived;

class Zero extends Base {
    constructor() {
        super();
    }
}

class Optional extends Base {
    constructor(seed?: number) {
        super();
    }
}

const zero: typeof Base = Zero;
const optional: typeof Base = Optional;
