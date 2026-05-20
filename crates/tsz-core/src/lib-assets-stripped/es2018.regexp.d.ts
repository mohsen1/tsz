interface RegExpMatchArray {
    groups?: {
        [key: string]: string;
    };
}
interface RegExpExecArray {
    groups?: {
        [key: string]: string;
    };
}
interface RegExp {
    readonly dotAll: boolean;
}
