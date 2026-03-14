interface SymbolConstructor {
    readonly prototype: Symbol;
    (description?: string | number): symbol;
    for(key: string): symbol;
    keyFor(sym: symbol): string | undefined;
}
declare var Symbol: SymbolConstructor;
