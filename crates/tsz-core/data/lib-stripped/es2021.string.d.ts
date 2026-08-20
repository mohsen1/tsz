interface String {
    replaceAll(searchValue: string | RegExp, replaceValue: string): string;
    replaceAll(searchValue: string | RegExp, replacer: (substring: string, ...args: any[]) => string): string;
}
