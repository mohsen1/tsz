import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execSync } from "node:child_process";
import {
  computeLocSplit,
  fmt,
  isTestFile,
  scanRust,
} from "../src/_data/loc.js";

function check(name, fn) {
  try {
    fn();
    console.log(`  ok  ${name}`);
  } catch (err) {
    console.error(`  FAIL ${name}`);
    throw err;
  }
}

console.log("loc-smoke: isTestFile");

check("integration tests dir is test", () => {
  assert.equal(isTestFile("crates/foo/tests/bar.rs"), true);
  assert.equal(isTestFile("crates/foo/tests/sub/baz.rs"), true);
});

check("tests.rs sibling is test", () => {
  assert.equal(isTestFile("crates/foo/src/state/tests.rs"), true);
});

check("_tests.rs suffix is test", () => {
  assert.equal(isTestFile("crates/foo/src/assignability/checker_tests.rs"), true);
  assert.equal(isTestFile("crates/foo/src/foo_test.rs"), true);
});

check("test_ prefix is test", () => {
  assert.equal(isTestFile("crates/foo/src/test_utils.rs"), true);
});

check("tests_ prefix is test", () => {
  assert.equal(isTestFile("crates/foo/src/tests_completions.rs"), true);
  assert.equal(isTestFile("crates/foo/src/bin/server/tests_navigation.rs"), true);
});

check("regular source files are not test", () => {
  assert.equal(isTestFile("crates/tsz-binder/src/lib.rs"), false);
  assert.equal(isTestFile("crates/tsz-binder/src/symbols.rs"), false);
  assert.equal(isTestFile("crates/foo/build.rs"), false);
});

check("benches dir is test", () => {
  assert.equal(isTestFile("crates/foo/benches/bar.rs"), true);
});

console.log("loc-smoke: scanRust newline & comment handling");

check("ignores cfg(test) inside line comments", () => {
  const src = [
    "fn a() {}",
    "// example: #[cfg(test)] mod tests { fn t() {} }",
    "fn b() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 3);
  assert.equal(testNl, 0);
});

check("ignores cfg(test) inside block comments and counts inner newlines as total", () => {
  const src = [
    "fn a() {}",
    "/* historical:",
    "#[cfg(test)]",
    "mod tests { fn t() {} }",
    "*/",
    "fn b() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 6);
  assert.equal(testNl, 0);
});

check("nested block comments do not desync the scanner", () => {
  const src = [
    "fn a() {}",
    "/* outer /* nested { } */ still comment */",
    "#[cfg(test)]",
    "mod tests {",
    "    fn t() {}",
    "}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 6);
  assert.equal(testNl, 4);
});

check("string literals can hide braces and cfg attrs", () => {
  const src = [
    "fn a() {",
    "    let s = \"#[cfg(test)] mod tests { fn t() {} }\";",
    "}",
    "fn b() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 4);
  assert.equal(testNl, 0);
});

check("raw strings with hashes do not confuse scanning", () => {
  const src = [
    "fn a() {",
    "    let s = r#\"contains \\\" and { } and #[cfg(test)] tokens\"#;",
    "}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 3);
  assert.equal(testNl, 0);
});

check("byte raw strings br#\"...\"#", () => {
  const src = [
    "fn a() {",
    "    let s = br#\"#[cfg(test)] mod tests { }\"#;",
    "}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("char literals like '{' do not unbalance braces", () => {
  const src = [
    "fn a() {",
    "    let c = '{';",
    "    let d = '}';",
    "}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("escape-char literals like '\\\\' and '\\''", () => {
  const src = [
    "fn a() {",
    "    let c = '\\\\';",
    "    let d = '\\'';",
    "}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 4);
  assert.equal(testNl, 0);
});

check("lifetimes ('a, 'static) are not treated as char literals", () => {
  const src = [
    "fn f<'a>(x: &'a str) -> &'static str { x; \"hi\" }",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 1);
  assert.equal(testNl, 0);
});

console.log("loc-smoke: scanRust inline cfg(test)");

check("counts inline mod tests block including delimiter lines", () => {
  const src = [
    "fn a() {}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    #[test]",
    "    fn t() {}",
    "}",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 9);
  assert.equal(testNl, 5);
});

check("counts mod foo; reference (declaration only)", () => {
  const src = [
    "fn a() {}",
    "",
    "#[cfg(test)]",
    "mod tests;",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 2);
});

check("cfg(test) static with `[T; N]` array type terminates at real `;`", () => {
  const src = [
    "fn outer() {}",
    "",
    "#[cfg(test)]",
    "static SAMPLES: [i32; 4] = [1, 2, 3, 4];",
    "",
    "fn after() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 2);
});

check("cfg(test) const with paren grouping is not terminated by `;` inside `()`", () => {
  const src = [
    "fn outer() {}",
    "",
    "#[cfg(test)]",
    "fn helper() -> [i32; 4] {",
    "    [1, 2, 3, 4]",
    "}",
    "",
    "fn after() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 4);
});

check("cfg(test) item with `;` inside string terminates at real `;`", () => {
  const src = [
    "fn outer() {}",
    "",
    "#[cfg(test)]",
    "const SAMPLE: &str = \"a; b; c;\";",
    "",
    "fn after() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 2);
});

check("cfg(test) item with `}` inside string does not exit early", () => {
  const src = [
    "fn outer() {}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    fn t() {",
    "        let s = \"} not real\";",
    "    }",
    "}",
    "",
    "fn after() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 6);
});

check("counts test block sandwiched between additional attributes", () => {
  const src = [
    "fn a() {}",
    "",
    "#[cfg(test)]",
    "#[path = \"../tests/foo.rs\"]",
    "mod tests;",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 3);
});

check("recognizes cfg(all(test, debug_assertions))", () => {
  const src = [
    "fn a() {}",
    "",
    "#[cfg(all(test, debug_assertions))]",
    "mod tests {",
    "    fn t() {}",
    "}",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 4);
});

check("does not match cfg(not(test))", () => {
  const src = [
    "#[cfg(not(test))]",
    "mod prod {",
    "    fn p() {}",
    "}",
    "",
    "fn a() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("does not match cfg(testing) (not the test ident)", () => {
  const src = [
    "#[cfg(testing)]",
    "mod stuff {",
    "    fn t() {}",
    "}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("counts bare #[test] function as test region", () => {
  const src = [
    "fn a() {}",
    "",
    "#[test]",
    "fn t() {",
    "    assert!(true);",
    "}",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 8);
  assert.equal(testNl, 4);
});

check("counts #[tokio::test] path attribute as test region", () => {
  const src = [
    "fn a() {}",
    "",
    "#[tokio::test]",
    "async fn t() {",
    "    do_thing().await;",
    "}",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 4);
});

check("counts #[test] with arg list like #[test(skip)]", () => {
  const src = [
    "fn a() {}",
    "",
    "#[test(skip)]",
    "fn t() {}",
    "",
    "fn b() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 2);
});

check("does not match #[test_case] (last segment must be exactly `test`)", () => {
  const src = [
    "#[test_case(1, 2 ; \"one\")]",
    "fn parametric(a: u32, b: u32) {}",
    "",
    "fn a() {}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("does not match #[test] inside string literal", () => {
  const src = [
    "fn a() {",
    "    let s = \"#[test]\\nfn t() {}\";",
    "}",
    "",
  ].join("\n");
  const { testNl } = scanRust(src);
  assert.equal(testNl, 0);
});

check("does not double-count nested cfg(test) inside an outer cfg(test)", () => {
  const src = [
    "#[cfg(test)]",
    "mod outer {",
    "    #[cfg(test)]",
    "    mod inner {",
    "        fn t() {}",
    "    }",
    "}",
    "",
    "fn a() {}",
    "",
  ].join("\n");
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 9);
  assert.equal(testNl, 7);
});

check("handles empty source", () => {
  const { totalNl, testNl } = scanRust("");
  assert.equal(totalNl, 0);
  assert.equal(testNl, 0);
});

check("handles file without trailing newline", () => {
  const src = "fn a() {}\nfn b() {}";
  const { totalNl, testNl } = scanRust(src);
  assert.equal(totalNl, 1);
  assert.equal(testNl, 0);
});

console.log("loc-smoke: computeLocSplit (integration)");

const tmp = await fs.mkdtemp(path.join(os.tmpdir(), "tsz-loc-smoke-"));
try {
  execSync("git init -q", { cwd: tmp });
  execSync("git config user.email smoke@example.com", { cwd: tmp });
  execSync("git config user.name smoke", { cwd: tmp });

  const crates = path.join(tmp, "crates");
  const crateA = path.join(crates, "crate-a");
  const crateASrc = path.join(crateA, "src");
  await fs.mkdir(crateASrc, { recursive: true });

  const libContent = [
    "pub fn one() -> u32 { 1 }",
    "pub fn two() -> u32 { 2 }",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    use super::*;",
    "    #[test]",
    "    fn t_one() {",
    "        assert_eq!(one(), 1);",
    "    }",
    "}",
    "",
  ].join("\n");
  const libNl = (libContent.match(/\n/g) || []).length;

  const sharedTestsContent = ["#[test]", "fn shared() {}", ""].join("\n");
  const libBContent = ["pub fn hello() -> &'static str { \"hi\" }", ""].join("\n");
  const buildContent = ["fn main() {}", ""].join("\n");
  // Pluralized `tests_` prefix: whole file should be classified test.
  const testsPluralContent = [
    "use super::*;",
    "",
    "#[test]",
    "fn pluralized_prefix() { assert!(true); }",
    "",
  ].join("\n");
  // Integration tests file under `tests/` directory (currently missed by globs).
  const integrationTestContent = [
    "#[test]",
    "fn integration() {}",
    "",
  ].join("\n");
  // Bench file under `benches/` directory.
  const benchContent = [
    "fn bench_main() {}",
    "",
  ].join("\n");
  const countNl = (s) => (s.match(/\n/g) || []).length;

  await fs.writeFile(path.join(crateASrc, "lib.rs"), libContent);
  await fs.writeFile(path.join(crateASrc, "tests.rs"), sharedTestsContent);
  await fs.writeFile(path.join(crateASrc, "tests_navigation.rs"), testsPluralContent);

  const crateAIntegrationDir = path.join(crateA, "tests");
  await fs.mkdir(crateAIntegrationDir, { recursive: true });
  await fs.writeFile(
    path.join(crateAIntegrationDir, "integration_tests.rs"),
    integrationTestContent,
  );

  const crateABenchDir = path.join(crateA, "benches");
  await fs.mkdir(crateABenchDir, { recursive: true });
  await fs.writeFile(path.join(crateABenchDir, "throughput.rs"), benchContent);

  const crateB = path.join(crates, "crate-b");
  await fs.mkdir(path.join(crateB, "src"), { recursive: true });
  await fs.writeFile(path.join(crateB, "src", "lib.rs"), libBContent);
  await fs.writeFile(path.join(crateB, "build.rs"), buildContent);

  execSync("git add -A", { cwd: tmp });
  execSync("git -c commit.gpgsign=false commit -q -m init", { cwd: tmp });

  const split = computeLocSplit(tmp);

  assert.equal(split.crate_count, 2, "crate count");
  assert.equal(
    split.total_lines,
    split.source_lines + split.test_lines,
    "total equals source + test",
  );

  const libInlineTest = scanRust(libContent).testNl;
  const expectedTestLines =
    libInlineTest +
    countNl(sharedTestsContent) +
    countNl(testsPluralContent) +
    countNl(integrationTestContent) +
    countNl(benchContent);

  assert.equal(
    split.test_lines,
    expectedTestLines,
    "test lines: inline + tests.rs + tests_*.rs + tests/*.rs + benches/*.rs",
  );
  assert.equal(
    split.source_lines,
    libNl - libInlineTest + countNl(libBContent) + countNl(buildContent),
    "source lines: libContent non-test + crate-b lib.rs + build.rs",
  );

  assert.equal(split.num_crates, "2");
  assert.equal(split.total_loc, fmt(split.total_lines));
  assert.equal(split.source_loc, fmt(split.source_lines));
  assert.equal(split.test_loc, fmt(split.test_lines));
} finally {
  await fs.rm(tmp, { recursive: true, force: true });
}

console.log("loc-smoke: all checks passed");
