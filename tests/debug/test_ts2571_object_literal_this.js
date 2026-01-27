const {  } = f();
const { p1 } = f();
const [] = f();
const [e1, e2] = f();
function testPropertyAccess(obj) {
    const value = obj.property;
}
class Foo {
     = 1;
    test(v) {
        const result =   v;
    }
}
;
;
const funcs1 = {
    funcA: (a) => {
    },
    funcB: (b, bb) => {
    }
};
const {  } = useReduxDispatch1((d, f) => ({
    funcA: (...p) => d(f.funcA(...p)),
    funcB: (...p) => d(f.funcB(...p))
}));
