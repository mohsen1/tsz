#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const { patchSessionClient } = require("./runner-session-client.cjs");
const patchTestState = require("./test-worker-patch-test-state.cjs");
const runner = require("./runner.cjs");

let failures = 0;
function test(name, fn) {
    try {
        fn();
        console.log(`  PASS  ${name}`);
    } catch (error) {
        failures++;
        console.error(`  FAIL  ${name}`);
        console.error(`    ${error.message}`);
    }
}

class FakeSessionClient {
    constructor() {
        this.getCombinedCodeFix = () => "constructor fallback";
        this.applyCodeActionCommand = () => "constructor fallback";
        this.mapCode = () => "constructor fallback";
    }

    writeMessage(message) {
        this.lastMessage = message;
        return "written";
    }

    getCompletionsAtPosition() {
        if (this.completionError) throw this.completionError;
        return this.completionResult;
    }

    getCompletionEntryDetails() {
        return this.detailResult;
    }

    getCodeFixesAtPosition() {
        return this.codeFixResult;
    }

    getFormattingEditsForRange() {
        return this.formatResult;
    }

    createFileLocationRequestArgs(fileName, position) {
        return { file: fileName, position };
    }

    processRequest(command, arguments_) {
        return { command, arguments: arguments_ };
    }

    processResponse(request) {
        this.lastRequest = request;
        return { body: this.responseBody };
    }

    decodeSpan(span, fileName) {
        return { decodedFor: fileName, start: span.start, end: span.end };
    }
}

console.log("session-client-truth.test.cjs");

test("canonical patch preserves TSZ completion, detail, fix, and formatting payloads", () => {
    const originalMethods = Object.fromEntries([
        "getCompletionsAtPosition",
        "getCompletionEntryDetails",
        "getCodeFixesAtPosition",
        "getFormattingEditsForRange",
    ].map(name => [name, FakeSessionClient.prototype[name]]));

    patchSessionClient(FakeSessionClient);

    for (const [name, original] of Object.entries(originalMethods)) {
        assert.strictEqual(FakeSessionClient.prototype[name], original, `${name} was replaced`);
    }

    const client = new FakeSessionClient();
    client.completionResult = { entries: [{ name: "from-tsz" }] };
    client.detailResult = { name: "from-tsz-detail" };
    client.codeFixResult = [{ fixName: "from-tsz-fix" }];
    client.formatResult = [{ newText: "from-tsz-format" }];
    client._languageService = {
        getCompletionsAtPosition: () => ({ entries: [{ name: "from-typescript" }] }),
    };

    assert.strictEqual(client.getCompletionsAtPosition(), client.completionResult);
    assert.strictEqual(client.getCompletionEntryDetails(), client.detailResult);
    assert.strictEqual(client.getCodeFixesAtPosition(), client.codeFixResult);
    assert.strictEqual(client.getFormattingEditsForRange(), client.formatResult);
});

test("definition decoders preserve metadata and context returned by tsz-server", () => {
    const client = new FakeSessionClient();
    client.responseBody = [{
        file: "/project/definition.ts",
        start: { line: 2, offset: 7 },
        end: { line: 2, offset: 11 },
        contextStart: { line: 2, offset: 1 },
        contextEnd: { line: 4, offset: 2 },
        kind: "class",
        name: "Tree",
        containerKind: "module",
        containerName: "Forest",
        isLocal: true,
        isAmbient: false,
        unverified: false,
        failedAliasResolution: false,
    }];

    assert.deepEqual(client.getTypeDefinitionAtPosition("/project/use.ts", 4), [{
        fileName: "/project/definition.ts",
        textSpan: {
            decodedFor: "/project/definition.ts",
            start: { line: 2, offset: 7 },
            end: { line: 2, offset: 11 },
        },
        kind: "class",
        name: "Tree",
        containerKind: "module",
        containerName: "Forest",
        isLocal: true,
        isAmbient: false,
        unverified: false,
        failedAliasResolution: false,
        contextSpan: {
            decodedFor: "/project/definition.ts",
            start: { line: 2, offset: 1 },
            end: { line: 4, offset: 2 },
        },
    }]);
    assert.deepEqual(client.lastRequest, {
        command: "typeDefinition",
        arguments: { file: "/project/use.ts", position: 4 },
    });
});

test("definition decoders supply TypeScript's unknown containerKind fallback", () => {
    const client = new FakeSessionClient();
    const entry = {
        file: "/project/definition.ts",
        start: { line: 1, offset: 2 },
        end: { line: 1, offset: 6 },
        kind: "const",
        name: "leaf",
    };
    const expectedDefinition = {
        fileName: "/project/definition.ts",
        textSpan: {
            decodedFor: "/project/definition.ts",
            start: { line: 1, offset: 2 },
            end: { line: 1, offset: 6 },
        },
        kind: "const",
        name: "leaf",
        containerKind: "",
        containerName: "",
        isLocal: false,
        isAmbient: false,
        unverified: false,
        failedAliasResolution: false,
    };

    client.responseBody = [entry];
    const decoded = client.getDefinitionAtPosition("/project/use.ts", 3);
    assert.deepEqual(decoded, [expectedDefinition]);
    assert.equal(decoded[0].containerKind, "");

    client.responseBody = [entry];
    assert.deepEqual(
        client.getTypeDefinitionAtPosition("/project/use.ts", 3),
        [expectedDefinition],
    );

    client.responseBody = {
        definitions: [entry],
        textSpan: {
            start: { line: 3, offset: 4 },
            end: { line: 3, offset: 8 },
        },
    };
    assert.deepEqual(
        client.getDefinitionAndBoundSpan("/project/use.ts", 3),
        {
            definitions: [expectedDefinition],
            textSpan: {
                decodedFor: "/project/use.ts",
                start: { line: 3, offset: 4 },
                end: { line: 3, offset: 8 },
            },
        },
    );
});

test("empty bound-span products become undefined without hiding nonclaims", () => {
    const client = new FakeSessionClient();
    client._languageService = {
        getDefinitionAtPosition: () => [{ name: "oracle-definition" }],
        getTypeDefinitionAtPosition: () => [{ name: "oracle-type-definition" }],
        getDefinitionAndBoundSpan: () => ({ definitions: [{ name: "oracle-bound" }] }),
    };

    client.responseBody = [];
    assert.deepEqual(client.getDefinitionAtPosition("/project/use.ts", 2), []);

    client.responseBody = [];
    assert.deepEqual(client.getTypeDefinitionAtPosition("/project/use.ts", 2), []);

    client.responseBody = {
        definitions: [],
        textSpan: {
            start: { line: 1, offset: 1 },
            end: { line: 1, offset: 2 },
        },
    };
    assert.equal(client.getDefinitionAndBoundSpan("/project/use.ts", 2), undefined);

    client.processResponse = () => {
        throw new Error("TSZ definitionAndBoundSpan incomplete: deferred");
    };
    assert.throws(
        () => client.getDefinitionAndBoundSpan("/project/use.ts", 2),
        /TSZ definitionAndBoundSpan incomplete: deferred/,
    );
});

test("empty TSZ completion is not backfilled by a nonempty in-process result", () => {
    const client = new FakeSessionClient();
    client.completionError = new Error("Malformed response: Unexpected empty response body.");
    client._languageService = {
        getCompletionsAtPosition: () => ({ entries: [{ name: "oracle-only" }] }),
    };
    assert.throws(
        () => client.getCompletionsAtPosition(),
        /Unexpected empty response body/,
    );
});

test("malformed TSZ failure is not caught or replaced", () => {
    const client = new FakeSessionClient();
    client.completionError = new Error("Malformed response from tsz-server");
    client._languageService = {
        getCompletionsAtPosition: () => ({ entries: [{ name: "oracle-only" }] }),
    };
    assert.throws(
        () => client.getCompletionsAtPosition(),
        /Malformed response from tsz-server/,
    );
});

test("canonical patch does not mutate the protocol writer or constructor surface", () => {
    const originalWriteMessage = FakeSessionClient.prototype.writeMessage;
    const client = new FakeSessionClient();
    const originalFallbacks = {
        getCombinedCodeFix: client.getCombinedCodeFix,
        applyCodeActionCommand: client.applyCodeActionCommand,
        mapCode: client.mapCode,
    };
    assert.equal(client.writeMessage("request"), "written");
    assert.equal(client.lastMessage, "request");
    assert.strictEqual(FakeSessionClient.prototype.writeMessage, originalWriteMessage);
    for (const [name, original] of Object.entries(originalFallbacks)) {
        assert.strictEqual(client[name], original, name);
    }
});

test("test-state patch routes the adapter but never patches assertions", () => {
    class TestState {}
    const verifyCompletionsWorker = function() { throw new Error("completion mismatch"); };
    const verifyCodeFix = function() { throw new Error("fix mismatch"); };
    TestState.prototype.verifyCompletionsWorker = verifyCompletionsWorker;
    TestState.prototype.verifyCodeFix = verifyCodeFix;

    class Adapter {
        constructor(token, options) {
            this.token = token;
            this.options = options;
        }
    }

    patchTestState({ TestState }, Adapter);
    const state = new TestState();
    const adapter = state.getLanguageServiceAdapter(1, "token", { strict: true });
    assert.ok(adapter instanceof Adapter);
    assert.strictEqual(TestState.prototype.verifyCompletionsWorker, verifyCompletionsWorker);
    assert.strictEqual(TestState.prototype.verifyCodeFix, verifyCodeFix);
    assert.throws(() => state.verifyCompletionsWorker(), /completion mismatch/);
    assert.throws(() => state.verifyCodeFix(), /fix mismatch/);
});

test("xfail cannot make a canonical run exit successfully", () => {
    const counts = runner.summarizeResults([{ status: "xfail" }]);
    assert.equal(runner.reportedPassCount(counts), 0);
    assert.equal(runner.runFailedCount(counts), 1);
});

test("discovery includes every selected TypeScript row", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-fourslash-truth-"));
    try {
        fs.writeFileSync(path.join(tempDir, "codeFixClassImplementInterfaceNoTruncation.ts"), "");
        fs.mkdirSync(path.join(tempDir, "nested"));
        fs.writeFileSync(path.join(tempDir, "nested", "ordinary.ts"), "");
        fs.writeFileSync(path.join(tempDir, "nested", "jsx-row.tsx"), "");
        fs.writeFileSync(path.join(tempDir, "not-a-test.js"), "");
        assert.deepEqual(
            runner.discoverTests(tempDir, "").map(file => path.basename(file)).sort(),
            ["codeFixClassImplementInterfaceNoTruncation.ts", "jsx-row.tsx", "ordinary.ts"],
        );
    } finally {
        fs.rmSync(tempDir, { recursive: true, force: true });
    }
});

test("vacuous, missing, duplicate, extra, and unknown results fail closed", () => {
    assert.throws(() => runner.validateResultCoverage([], []), /No fourslash tests were selected/);
    assert.throws(
        () => runner.validateResultCoverage(["one.ts"], []),
        /no result for one\.ts/,
    );
    assert.throws(
        () => runner.validateResultCoverage(
            ["one.ts"],
            [{ file: "one.ts", status: "fail" }, { file: "one.ts", status: "fail" }],
        ),
        /duplicate results/,
    );
    assert.throws(
        () => runner.validateResultCoverage(["one.ts"], [{ file: "two.ts", status: "fail" }]),
        /does not belong/,
    );
    assert.throws(
        () => runner.validateResultCoverage(["one.ts"], [{ file: "one.ts", status: "mystery" }]),
        /unknown status/,
    );
    assert.doesNotThrow(() => runner.validateResultCoverage(
        ["one.ts", "two.ts"],
        [{ file: "one.ts", status: "pass" }, { file: "two.ts", status: "fail" }],
    ));
});

test("canonical sources contain no native-service or fixture arbitration route", () => {
    const root = __dirname;
    const canonicalFiles = [
        "runner-session-client.cjs",
        "runner.cjs",
        "test-worker-patch-test-state.cjs",
        "test-worker.cjs",
        "tsz-adapter.cjs",
    ];
    const forbidden = [
        "createLanguageService",
        "getNativeLanguageService",
        "withNativeFallback",
        "_tszNativeLs",
        "TEMP_PARITY_ALLOWLIST",
        "__tszCurrentFourslashTestFile",
        "synthesizedEdits",
        "skip_if_failing",
        "import-fix-parity-overrides",
        "preProcessFile",
        "seenForCall",
        "originalParseCustomTypeOption",
        "normalizedContent",
    ];
    for (const file of canonicalFiles) {
        const source = fs.readFileSync(path.join(root, file), "utf8");
        for (const token of forbidden) {
            assert.equal(source.includes(token), false, `${file} contains ${token}`);
        }
    }
    const parallelSource = fs.readFileSync(path.join(root, "test-worker.cjs"), "utf8");
    const sequentialSource = fs.readFileSync(path.join(root, "runner.cjs"), "utf8");
    for (const dependency of [
        'require("./runner-session-client.cjs")',
        'require("./test-worker-patch-test-state.cjs")',
    ]) {
        assert.equal(parallelSource.includes(dependency), true, `parallel worker lacks ${dependency}`);
        assert.equal(sequentialSource.includes(dependency), true, `sequential runner lacks ${dependency}`);
    }
    assert.equal(fs.existsSync(path.join(root, "test-worker-session-client-completions.cjs")), false);
    assert.equal(fs.existsSync(path.join(root, "test-worker-session-client-fixes.cjs")), false);
    assert.equal(fs.existsSync(path.join(root, "skip_if_failing.txt")), false);
    assert.equal(fs.existsSync(path.join(root, "import-fix-parity-overrides.cjs")), false);
});

test("sequential fixtures reset their server session and recover before continuing", () => {
    let resets = 0;
    let restarts = 0;
    const resetResult = runner.resetBridgeForNextTest({
        resetSession() {
            resets++;
        },
    }, () => {
        restarts++;
    });
    assert.equal(resetResult, undefined);
    assert.equal(resets, 1);
    assert.equal(restarts, 0);

    const resetError = new Error("reset failed");
    const recovered = Symbol("recovered");
    const recoveryResult = runner.resetBridgeForNextTest({
        resetSession() {
            throw resetError;
        },
    }, error => {
        assert.strictEqual(error, resetError);
        restarts++;
        return recovered;
    });
    assert.strictEqual(recoveryResult, recovered);
    assert.equal(restarts, 1);

    const sequentialSource = fs.readFileSync(path.join(__dirname, "runner.cjs"), "utf8");
    assert.match(
        sequentialSource,
        /finally\s*\{\s*await resetBridgeForNextTest\(bridge,[\s\S]*?restartBridge\(/,
    );
});

test("CI aggregation never turns failed shards or a tolerant count floor into success", () => {
    const fullCi = fs.readFileSync(path.join(__dirname, "..", "ci", "full-ci.sh"), "utf8");
    const start = fullCi.indexOf("run_fourslash_aggregate() {");
    const end = fullCi.indexOf("\nrun_dist_binaries() {", start);
    assert.ok(start >= 0 && end > start, "run_fourslash_aggregate body missing");
    const body = fullCi.slice(start, end);
    assert.equal(body.includes("fourslash-snapshot.json"), false);
    assert.equal(body.includes("tolerance"), false);
    assert.match(body, /failed_shards.*timed_out[\s\S]+return 1/);
    assert.match(body, /complete:\(\$failed_shards == 0 and \$timed_out == 0\)/);
});

if (failures > 0) process.exit(1);
