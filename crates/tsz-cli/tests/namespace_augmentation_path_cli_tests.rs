//! Nested module augmentations are selected by their full declaration path.
//!
//! A terminal interface name is not a sufficient identity: one augmentation
//! can contain `Entry`, `Container.Entry`, `Container.Deep.Entry`, and
//! `Sibling.Entry`. Each declaration merges only with the native interface at
//! the same namespace path, independent of declaration order or the local name
//! chosen for the namespace import.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

fn compile_fixture(surface: &str, augmentation: &str, consumer: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in [
        ("surface.ts", surface),
        ("augmentation.ts", augmentation),
        ("consumer.ts", consumer),
    ] {
        std::fs::write(dir.path().join(name), source).expect("write repro file");
    }

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "--lib",
        "es2022",
        "consumer.ts",
    ])
    .expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

const NATIVE_PATHS: &str = r#"
export namespace Container {
    export interface Entry {
        containerNative: "container-native";
    }

    export namespace Deep {
        export interface Entry {
            deepNative: "deep-native";
        }
    }
}

export namespace Sibling {
    export interface Entry {
        siblingNative: "sibling-native";
    }
}
"#;

const TOP_LEVEL_FIRST: &str = r#"
import "./surface";

declare module "./surface" {
    export interface Entry {
        topAugmented: "top-augmentation";
    }

    export namespace Container {
        export interface Entry {
            containerAugmented: "container-augmentation";
        }

        export namespace Deep {
            export interface Entry {
                deepAugmented: "deep-augmentation";
                next?: Entry;
                related: import("./surface").Sibling.Entry;
            }
        }
    }

    export namespace Sibling {
        export interface Entry {
            siblingAugmented: "sibling-augmentation";
        }
    }
}
"#;

const NESTED_FIRST: &str = r#"
import "./surface";

declare module "./surface" {
    export namespace Sibling {
        export interface Entry {
            siblingAugmented: "sibling-augmentation";
        }
    }

    export namespace Container {
        export namespace Deep {
            export interface Entry {
                deepAugmented: "deep-augmentation";
                next?: Entry;
                related: import("./surface").Sibling.Entry;
            }
        }

        export interface Entry {
            containerAugmented: "container-augmentation";
        }
    }

    export interface Entry {
        topAugmented: "top-augmentation";
    }
}
"#;

const PATH_CONSUMER: &str = r#"
import "./augmentation";
import * as renamedSurface from "./surface";

const top: renamedSurface.Entry = {
    topAugmented: "top-augmentation",
};
const container: renamedSurface.Container.Entry = {
    containerNative: "container-native",
    containerAugmented: "container-augmentation",
};
const deep: renamedSurface.Container.Deep.Entry = {
    deepNative: "deep-native",
    deepAugmented: "deep-augmentation",
    related: {
        siblingNative: "sibling-native",
        siblingAugmented: "sibling-augmentation",
    },
};
const sibling: renamedSurface.Sibling.Entry = {
    siblingNative: "sibling-native",
    siblingAugmented: "sibling-augmentation",
};

top.topAugmented;
container.containerNative;
container.containerAugmented;
deep.deepNative;
deep.deepAugmented;
deep.next?.deepNative;
deep.next?.deepAugmented;
deep.related.siblingNative;
deep.related.siblingAugmented;
sibling.siblingNative;
sibling.siblingAugmented;
"#;

#[test]
fn same_terminal_interfaces_stay_on_their_exact_paths_in_both_orders() {
    for (order, augmentation) in [
        ("top-level-first", TOP_LEVEL_FIRST),
        ("nested-first", NESTED_FIRST),
    ] {
        let diagnostics = compile_fixture(NATIVE_PATHS, augmentation, PATH_CONSUMER);
        assert!(
            diagnostics.is_empty(),
            "{order}: direct and nested `Entry` declarations must merge only at their exact paths; got: {diagnostics:?}"
        );
    }
}

#[test]
fn generic_native_and_augmented_interfaces_remain_path_scoped() {
    let diagnostics = compile_fixture(
        r#"
export interface GenericEntry<T> {
    topNative: T;
}

export namespace Container {
    export interface GenericEntry<T> {
        containerNative: T;
    }

    export namespace Deep {
        export interface GenericEntry<T> {
            deepNative: T;
        }
    }
}

export namespace Sibling {
    export interface GenericEntry<T> {
        siblingNative: T;
    }
}
"#,
        r#"
import "./surface";

declare module "./surface" {
    export interface GenericEntry<T> {
        topAugmented: T;
    }

    export namespace Container {
        export interface GenericEntry<T> {
            containerAugmented: T;
        }

        export namespace Deep {
            export interface GenericEntry<T> {
                deepAugmented: T;
            }
        }
    }

    export namespace Sibling {
        export interface GenericEntry<T> {
            siblingAugmented: T;
        }
    }
}
"#,
        r#"
import "./augmentation";
import * as renamedGenericSurface from "./surface";

const top: renamedGenericSurface.GenericEntry<number> = {
    topNative: 1,
    topAugmented: 2,
};
const container: renamedGenericSurface.Container.GenericEntry<number> = {
    containerNative: 1,
    containerAugmented: 2,
};
const deep: renamedGenericSurface.Container.Deep.GenericEntry<number> = {
    deepNative: 1,
    deepAugmented: 2,
};
const sibling: renamedGenericSurface.Sibling.GenericEntry<number> = {
    siblingNative: 1,
    siblingAugmented: 2,
};

top.topAugmented;
container.containerAugmented;
deep.deepAugmented;
sibling.siblingAugmented;

const wrong: renamedGenericSurface.Container.GenericEntry<number> = {
    containerNative: "wrong",
    containerAugmented: 2,
};
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2322],
        "generic native and augmentation declarations must remain separated by namespace path while preserving the real negative assignment; got: {diagnostics:?}"
    );
}

#[test]
fn generic_constraints_and_defaults_use_the_exact_path_parameters() {
    let diagnostics = compile_fixture(
        r#"
export interface DefaultedEntry<
    T extends { id: number } = { id: number },
    U = T,
> {
    topNative: T;
    topPeer: U;
}

export namespace Container {
    export interface DefaultedEntry<
        T extends { id: number } = { id: number },
        U = T,
    > {
        containerNative: T;
        containerPeer: U;
    }
}
"#,
        r#"
import "./surface";

declare module "./surface" {
    export interface DefaultedEntry<
        T extends { id: number } = { id: number },
        U = T,
    > {
        topAugmented: T;
    }

    export namespace Container {
        export interface DefaultedEntry<
            T extends { id: number } = { id: number },
            U = T,
        > {
            containerAugmented: U;
        }
    }
}
"#,
        r#"
import "./augmentation";
import * as defaultedSurface from "./surface";

const bare: defaultedSurface.Container.DefaultedEntry = {
    containerNative: { id: 1 },
    containerPeer: { id: 2 },
    containerAugmented: { id: 3 },
};
const partial: defaultedSurface.Container.DefaultedEntry<{ id: 1 }> = {
    containerNative: { id: 1 },
    containerPeer: { id: 1 },
    containerAugmented: { id: 1 },
};

bare.containerAugmented.id;
partial.containerPeer.id;

let invalid!: defaultedSurface.Container.DefaultedEntry<string>;
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2344],
        "exact-path generic applications must fill omitted defaults and validate the selected declaration's constraint; got: {diagnostics:?}"
    );
}
