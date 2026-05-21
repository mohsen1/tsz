export {};

type Schema = {
    user: {
        id: number;
        name: string;
    };
    post: {
        id: number;
        userId: number;
    };
};

declare function selectFrom<S, K extends keyof S>(table: K): {
    select<P extends keyof S[K]>(column: P): S[K][P];
};

const id = selectFrom<Schema, "user">("user").select("id");
const idCheck: number = id;

const name = selectFrom<Schema, "user">("user").select("name");
const nameCheck: string = name;
