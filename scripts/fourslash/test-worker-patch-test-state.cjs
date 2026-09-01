"use strict";

// Route the upstream fourslash state to the TSZ protocol adapter. Do not patch
// assertion methods, manufacture Program/SourceFile handles, or special-case
// fixture names: an unsupported service operation is a canonical failure.
module.exports = function patchTestState(FourSlash, TszAdapter) {
    const TestState = FourSlash?.TestState;
    if (!TestState) throw new Error("Could not find TestState in FourSlash module");

    TestState.prototype.getLanguageServiceAdapter = function(
        _testType,
        cancellationToken,
        compilationOptions,
    ) {
        return new TszAdapter(cancellationToken, compilationOptions);
    };
};
