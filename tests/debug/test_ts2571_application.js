const funcs1 = {
    funcA: (a) => {
    },
    funcB: (b) => {
    }
};
const result = useDestructuring((f) => ({
    funcA: (...p) => f.funcA(...p),
    funcB: (...p) => f.funcB(...p)
}));
