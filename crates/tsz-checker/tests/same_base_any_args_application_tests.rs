//! Same-definition all-`any`-args application assignability.
//!
//! When a generic class/alias instantiation `X<DB, TB>` is passed where
//! `X<any, any>` is expected, tsc relates them under assignability (any
//! propagation + same-reference comparison) even when the structural member
//! comparison would stumble on generic-method constraints. The witness family
//! is kysely's `Kysely<DB>` -> `Kysely<any>` /
//! `Readonly<FilterObject<DB, TB>>` -> `Readonly<FilterObject<any, any>>`
//! call-argument sites.
//!
//! Structural rule: when source and target are applications of the SAME
//! definition (or the source is that definition's own instance type) and
//! every target argument is `any`, the pair is related under assignability;
//! different-base targets and concrete-argument targets keep the structural
//! path.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic: Diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn same_class_instance_assignable_to_all_any_args_target() {
    let diagnostics = codes(
        r#"
declare function consume(db: Repo<any>): void

class Repo<DB> {
  pickFrom<TB extends keyof DB & string>(from: TB): TB {
    return undefined as any
  }
  detach(): Repo<DB> {
    return this
  }
  go() {
    consume(this.detach())
  }
}
"#,
    );
    assert!(
        !diagnostics.contains(&2345),
        "Repo<DB> must be assignable to Repo<any> (same definition, all-any target args), got {diagnostics:?}"
    );
}

#[test]
fn same_class_instance_assignable_to_all_any_args_target_renamed_binders() {
    let diagnostics = codes(
        r#"
declare function sink(value: Holder<any>): void

class Holder<Inner> {
  grab<Key extends keyof Inner & string>(key: Key): Key {
    return undefined as any
  }
  fresh(): Holder<Inner> {
    return this
  }
  run() {
    sink(this.fresh())
  }
}
"#,
    );
    assert!(
        !diagnostics.contains(&2345),
        "renamed binders: Holder<Inner> must be assignable to Holder<any>, got {diagnostics:?}"
    );
}

#[test]
fn same_class_with_heritage_assignable_to_all_any_args_target() {
    let diagnostics = codes(
        r#"
interface Introspector {
  getTables(): string[]
}

interface Dialect {
  createIntrospector(db: Repo<any>): Introspector
}

interface RepoProps {
  readonly dialect: Dialect
}

type AnyColumn<DB, TB extends keyof DB> = {
  [T in TB]: keyof DB[T] & string
}[TB]

class Creator<DB> {
  pickFrom<TB extends keyof DB & string>(from: TB): AnyColumn<DB, TB> {
    return undefined as any
  }
  withSchema(schema: string): Creator<DB> {
    return this
  }
}

class Repo<DB> extends Creator<DB> {
  readonly #props: RepoProps

  constructor(props: RepoProps) {
    super()
    this.#props = props
  }

  get introspection(): Introspector {
    return this.#props.dialect.createIntrospector(this.detach())
  }

  detach(): Repo<DB> {
    return new Repo(this.#props)
  }
}
"#,
    );
    assert!(
        !diagnostics.contains(&2345),
        "heritage case: Repo<DB> must be assignable to Repo<any>, got {diagnostics:?}"
    );
}

#[test]
fn different_base_all_any_args_target_stays_structural() {
    // tsc 5.9.3 rejects this: the all-`any` shortcut is same-reference only;
    // a different base with an identical shape keeps the structural path.
    let diagnostics = codes(
        r#"
declare function consume(db: Other<any>): void

class Repo<DB> {
  pickFrom<TB extends keyof DB & string>(from: TB): TB {
    return undefined as any
  }
  detach(): Repo<DB> {
    return this
  }
  go() {
    consume(this.detach())
  }
}

class Other<DB> {
  pickFrom<TB extends keyof DB & string>(from: TB): TB {
    return undefined as any
  }
  detach(): Other<DB> {
    return this
  }
}
"#,
    );
    assert!(
        diagnostics.contains(&2345),
        "different-base all-any target must stay structural (tsc errors here), got {diagnostics:?}"
    );
}

#[test]
fn all_any_source_to_concrete_target_unchanged() {
    // Reverse direction: `Repo<any>` -> `Repo<{ a: 1 }>` is accepted via any
    // propagation on the SOURCE arguments (tsc-clean); the target shortcut
    // must not disturb it.
    let diagnostics = codes(
        r#"
class Repo<DB> {
  pickFrom<TB extends keyof DB & string>(from: TB): TB {
    return undefined as any
  }
  detach(): Repo<DB> {
    return this
  }
}

declare const anyRepo: Repo<any>
const concrete: Repo<{ a: 1 }> = anyRepo
"#,
    );
    assert!(
        !diagnostics.contains(&2322),
        "Repo<any> must remain assignable to Repo<{{ a: 1 }}>, got {diagnostics:?}"
    );
}

#[test]
fn union_alias_all_any_args_target_stays_clean() {
    let diagnostics = codes(
        r#"
interface DynRef<R> {
  readonly dynamicReference: R
}

type AnyColumn<DB, TB extends keyof DB> = {
  [T in TB]: keyof DB[T] & string
}[TB]

type StringReference<DB, TB extends keyof DB> = AnyColumn<DB, TB> | DynRef<any>

type ReferenceOrList<DB, TB extends keyof DB> =
  | ReadonlyArray<StringReference<DB, TB>>
  | StringReference<DB, TB>

declare function parseReference(reference: ReferenceOrList<any, any>): void

class Builder<DB, TB extends keyof DB> {
  partitionBy(reference: ReferenceOrList<DB, TB>): void {
    parseReference(reference)
  }
}
"#,
    );
    assert!(
        !diagnostics.contains(&2345),
        "union alias instantiation must be assignable to its all-any instantiation, got {diagnostics:?}"
    );
}
