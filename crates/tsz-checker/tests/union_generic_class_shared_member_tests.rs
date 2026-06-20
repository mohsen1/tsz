//! Property access on a union of *generic* class/interface instances reached
//! through a deferred `IndexAccess` must credit a member that is present on
//! every union arm, even when one arm stays an unresolved
//! `Application(Lazy(DefId), args)` that the property-access boundary's noop
//! `TypeResolver` cannot crack.
//!
//! Regression for #14128: `(input & Set<any>) | (UnknownInput & Set<any>)`
//! (from `value instanceof Set` narrowing of `input | UnknownInput`) reported
//! a false `TS2339` on `.size` because one `Set<any>` arm remained an
//! unresolved `Application`, whose member lookup degraded to `PropertyNotFound`
//! and collapsed the whole union. The minimized, deterministic witness uses
//! generic classes behind a `Slices[SliceId][SliceKey]` indexed access so the
//! union arms stay deferred `Application`s under the test harness.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

/// A method present on *both* generic-class arms of a deferred indexed-access
/// union must resolve — no `TS2339`. Names are deliberately varied (`Alpha`/
/// `Beta`, `read`) so the fix cannot be a name-keyed special case.
///
/// The two-class shape (`Simple` *and* `Complex`) is load-bearing: it keeps one
/// `Alpha<number>`/`Beta<string>` arm as an unresolved `Application(Lazy, args)`
/// in the boundary's noop-resolver path, which is exactly the state that made
/// the pre-fix member lookup degrade to a false `PropertyNotFound`.
#[test]
fn shared_member_on_union_of_generic_class_applications_is_not_missing() {
    let source = r#"
class Alpha<T extends number> {
  private value!: T;
  shared(): T { return this.value; }
  onlyAlpha(): void {}
}
class Beta<T extends string> {
  private value!: T;
  shared(): T { return this.value; }
  onlyBeta(): void {}
}
const isAlpha = <Candidate extends Alpha<number> | Beta<string>>(
  candidate: Candidate
): candidate is Extract<Candidate, Alpha<any>> => candidate instanceof Alpha;

class Simple<Entries extends { [index: string]: Alpha<number> | Beta<string> }> {
  private entries = {} as Entries;
  read<EntryId extends keyof Entries>(entryId: EntryId): Entries[EntryId] {
    let entry = this.entries[entryId];
    if (isAlpha(entry)) {
      return entry;
    }
    return entry;
  }
}

type Slice = { [index: string]: Alpha<number> | Beta<string> };
class Complex<Slices extends { [index: string]: Slice }> {
  private slices = {} as Slices;
  read<SliceId extends keyof Slices, SliceKey extends keyof Slices[SliceId]>(
    sliceId: SliceId,
    sliceKey: SliceKey
  ): Slices[SliceId][SliceKey] {
    let item = this.slices[sliceId][sliceKey];
    if (isAlpha(item)) {
      item.onlyAlpha();
    }
    item.shared();
    return item;
  }
}
"#;
    let diags = check(source);
    assert!(
        !diags
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("shared")),
        "`shared` exists on every union arm; it must not report TS2339. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// A member present on only *one* arm must still report `TS2339` — the rescue
/// must not blanket-suppress genuine missing-property errors on the same shape.
#[test]
fn member_on_single_arm_of_generic_class_union_still_reports_ts2339() {
    let source = r#"
class Alpha<T extends number> {
  private value!: T;
  shared(): T { return this.value; }
  onlyAlpha(): void {}
}
class Beta<T extends string> {
  private value!: T;
  shared(): T { return this.value; }
  onlyBeta(): void {}
}
const isAlpha = <Candidate extends Alpha<number> | Beta<string>>(
  candidate: Candidate
): candidate is Extract<Candidate, Alpha<any>> => candidate instanceof Alpha;

class Simple<Entries extends { [index: string]: Alpha<number> | Beta<string> }> {
  private entries = {} as Entries;
  read<EntryId extends keyof Entries>(entryId: EntryId): Entries[EntryId] {
    let entry = this.entries[entryId];
    if (isAlpha(entry)) {
      return entry;
    }
    return entry;
  }
}

type Slice = { [index: string]: Alpha<number> | Beta<string> };
class Complex<Slices extends { [index: string]: Slice }> {
  private slices = {} as Slices;
  read<SliceId extends keyof Slices, SliceKey extends keyof Slices[SliceId]>(
    sliceId: SliceId,
    sliceKey: SliceKey
  ): Slices[SliceId][SliceKey] {
    let item = this.slices[sliceId][sliceKey];
    item.onlyAlpha();
    return item;
  }
}
"#;
    let diags = check(source);
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("onlyAlpha")),
        "`onlyAlpha` is absent on the `Beta` arm; TS2339 must still fire. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
