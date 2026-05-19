"""Behavior-lock unit tests for the emit failure-family classifier in query-emit.py."""

import importlib.util
import sys
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).with_name("query-emit.py")
_SPEC = importlib.util.spec_from_file_location("query_emit", _SCRIPT)
query_emit = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
sys.modules[_SPEC.name] = query_emit
_SPEC.loader.exec_module(query_emit)


def _js_result(name: str, error: str = "") -> dict:
    return {"name": name, "testPath": "", "baselineFile": "", "jsError": error, "jsStatus": "fail"}


def _dts_result(name: str, error: str = "") -> dict:
    return {"name": name, "testPath": "", "baselineFile": "", "dtsError": error, "dtsStatus": "fail"}


def js_family(name: str, error: str = "") -> str:
    return query_emit.classify_failure(_js_result(name, error), "js")


def dts_family(name: str, error: str = "") -> str:
    return query_emit.classify_failure(_dts_result(name, error), "dts")


class TestJsParserFamily(unittest.TestCase):
    def test_parser_prefix(self):
        self.assertEqual(js_family("parserComputedPropertyName25"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parserRealSource10"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parserSkippedTokens8"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parserharness"), "parser/syntax recovery emit")

    def test_parse_variants_without_r(self):
        # Names like parseBigInt start with "parse" but not "parser"
        self.assertEqual(js_family("parseBigInt"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parseErrorIncorrectReturnToken"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parseAssertEntriesError"), "parser/syntax recovery emit")
        self.assertEqual(js_family("parseInvalidNames"), "parser/syntax recovery emit")


class TestJsOptionalChainFamily(unittest.TestCase):
    def test_optional_chain_names(self):
        self.assertEqual(js_family("optionalChainingInArrow(target=es2015)"), "optional-chain/nullish lowering")
        self.assertEqual(js_family("invalidOptionalChainFromNewExpression"), "optional-chain/nullish lowering")
        # optionalChainingInLoop contains "loop" which fires the loop/control-flow rule first;
        # both classifications are structurally accurate for that test.
        self.assertEqual(js_family("optionalChainingInLoop(target=es5)"), "loop/control-flow emit")

    def test_access_chain_names(self):
        self.assertEqual(js_family("elementAccessChain.3"), "optional-chain/nullish lowering")
        self.assertEqual(js_family("propertyAccessChain.3"), "optional-chain/nullish lowering")


class TestJsArrowFunctionFamily(unittest.TestCase):
    def test_arrow_names(self):
        self.assertEqual(js_family("fatarrowfunctionsOptionalArgs"), "arrow-function emit")
        self.assertEqual(js_family("disallowLineTerminatorBeforeArrow"), "arrow-function emit")

    def test_arrowfunc_variant(self):
        self.assertEqual(js_family("arrowfuncWithDefaultParams"), "arrow-function emit")

    def test_does_not_displace_class_family(self):
        # "class" must still be classified as class lowering, not arrow
        self.assertEqual(js_family("classDecorator"), "class/private/accessor/decorator lowering")


class TestJsTypeGuardFamily(unittest.TestCase):
    def test_typeguard_names(self):
        self.assertEqual(js_family("typeGuardFunctionErrors"), "type-guard/narrowing emit")
        self.assertEqual(js_family("typeGuardOfFormThisMember(target=es5)"), "type-guard/narrowing emit")
        self.assertEqual(js_family("typeGuardsInConditionalExpression"), "type-guard/narrowing emit")
        self.assertEqual(js_family("typeGuardsInRightOperandOfAndAndOperator"), "type-guard/narrowing emit")


class TestJsUnicodeReservedFamily(unittest.TestCase):
    def test_unicode_names(self):
        self.assertEqual(js_family("invalidUnicodeEscapeSequance"), "unicode/reserved-word identifier emit")
        self.assertEqual(js_family("invalidUnicodeEscapeSequance4"), "unicode/reserved-word identifier emit")
        self.assertEqual(js_family("unicodeEscapesInNames02(target=es5)"), "unicode/reserved-word identifier emit")

    def test_reserved_word_names(self):
        self.assertEqual(js_family("reservedWords2"), "unicode/reserved-word identifier emit")
        self.assertEqual(js_family("reservedNamesInAliases"), "unicode/reserved-word identifier emit")


class TestJsJsFileFamily(unittest.TestCase):
    def test_jsfile_names(self):
        self.assertEqual(js_family("jsFileCompilationEmitTrippleSlashReference"), "jsdoc/js-file emit")
        self.assertEqual(js_family("jsFileCompilationTypeArgumentSyntaxOfCall"), "jsdoc/js-file emit")

    def test_jsdeclarations_names(self):
        self.assertEqual(js_family("jsDeclarationsNestedParams"), "jsdoc/js-file emit")
        self.assertEqual(js_family("jsDeclarationsTypeReferences4(target=es2015)"), "jsdoc/js-file emit")

    def test_plainjsgram_name(self):
        self.assertEqual(js_family("plainJSGrammarErrors"), "jsdoc/js-file emit")

    def test_jsdoc_name(self):
        self.assertEqual(js_family("expressionWithJSDocTypeArguments"), "jsdoc/js-file emit")


class TestJsParameterInitFamily(unittest.TestCase):
    def test_captured_param_names(self):
        self.assertEqual(
            js_family("capturedParametersInInitializers2(target=es2015)"),
            "parameter-init/captured-var emit",
        )
        self.assertEqual(
            js_family("capturedParametersInInitializers2(target=es5)"),
            "parameter-init/captured-var emit",
        )

    def test_parameter_initializer_name(self):
        self.assertEqual(
            js_family("functionLikeInParameterInitializer(target=es5)"),
            "parameter-init/captured-var emit",
        )

    def test_parameter_declaration_name(self):
        self.assertEqual(
            js_family("duplicateIdentifierBindingElementInParameterDeclaration1(target=es5)"),
            "parameter-init/captured-var emit",
        )


class TestJsTslibFamily(unittest.TestCase):
    def test_tslib_names(self):
        self.assertEqual(js_family("tslibMissingHelper"), "tslib/helper-import emit")
        self.assertEqual(js_family("tslibMultipleMissingHelper"), "tslib/helper-import emit")


class TestJsNewTargetFamily(unittest.TestCase):
    def test_newtarget_names(self):
        self.assertEqual(js_family("newTarget.es5(target=es5)"), "new.target emit")
        self.assertEqual(js_family("invalidNewTarget.es5(target=es5)"), "new.target emit")


class TestJsUnusedSymbolFamily(unittest.TestCase):
    def test_unused_local_names(self):
        self.assertEqual(js_family("unusedLocalsAndParameters"), "unused-symbol/visibility emit")
        self.assertEqual(js_family("unusedLocalsStartingWithUnderscore"), "unused-symbol/visibility emit")

    def test_isdeclarationvisible_name(self):
        self.assertEqual(
            js_family("isDeclarationVisibleNodeKinds(target=es5)"),
            "unused-symbol/visibility emit",
        )


class TestJsTypeAssertionFamily(unittest.TestCase):
    def test_typeassert_name(self):
        self.assertEqual(js_family("typeAssertions"), "type-assertion/instanceof emit")

    def test_instanceof_name(self):
        self.assertEqual(js_family("instanceofOperator(target=es5)"), "type-assertion/instanceof emit")

    def test_isolateddeclaration_name(self):
        self.assertEqual(js_family("isolatedDeclarationLazySymbols"), "type-assertion/instanceof emit")

    def test_ts_expect_error_name(self):
        self.assertEqual(js_family("ts-expect-error"), "type-assertion/instanceof emit")


class TestJsNestedFunctionFamily(unittest.TestCase):
    def test_local_function_names(self):
        self.assertEqual(
            js_family("methodContainingLocalFunction(target=es2015)"),
            "nested-function/local-scope emit",
        )
        self.assertEqual(
            js_family("methodContainingLocalFunction(target=es5)"),
            "nested-function/local-scope emit",
        )


class TestJsExtendedExistingFamilies(unittest.TestCase):
    def test_tsx_classified_as_jsx(self):
        self.assertEqual(js_family("tsxStatelessComponentDefaultProps"), "jsx/react emit")
        self.assertEqual(js_family("tsxUnionMemberChecksFilterDataProps"), "jsx/react emit")

    def test_loop_keyword_extends_loop_family(self):
        self.assertEqual(js_family("nestedLoops(target=es5)"), "loop/control-flow emit")
        self.assertEqual(
            js_family("newLexicalEnvironmentForConvertedLoop(target=es5)"),
            "loop/control-flow emit",
        )

    def test_system_extends_module_family(self):
        self.assertEqual(js_family("es5-system(target=es5)"), "module/import/export emit")

    def test_promise_extends_async_family(self):
        self.assertEqual(
            js_family("operationsAvailableOnPromisedType(target=es2015)"),
            "async/await/generator lowering",
        )
        self.assertEqual(
            js_family("operationsAvailableOnPromisedType(target=es5)"),
            "async/await/generator lowering",
        )

    def test_usebeforedecl_extends_block_scoping(self):
        self.assertEqual(
            js_family("useBeforeDeclaration_propertyAssignment"),
            "block-scoping/hoisting emit",
        )


class TestDtsJsFileFamily(unittest.TestCase):
    def test_jsfile_in_jsdoc_family(self):
        self.assertEqual(dts_family("jsFileAlternativeUseOfOverloadTag"), "jsdoc/javascript declarations")
        self.assertEqual(dts_family("jsFileCompilationDuplicateVariable"), "jsdoc/javascript declarations")
        self.assertEqual(dts_family("jsFileFunctionOverloads"), "jsdoc/javascript declarations")
        self.assertEqual(dts_family("jsFileMethodOverloads2"), "jsdoc/javascript declarations")

    def test_existing_jsdoc_names_still_work(self):
        self.assertEqual(dts_family("jsDocReturn"), "jsdoc/javascript declarations")
        self.assertEqual(dts_family("checkJsDirective"), "jsdoc/javascript declarations")


class TestDtsGenericExtended(unittest.TestCase):
    def test_tuple_names(self):
        self.assertEqual(dts_family("namedTupleMembers"), "generic/type-display declarations")
        self.assertEqual(dts_family("variadicTuples1"), "generic/type-display declarations")
        self.assertEqual(dts_family("variadicTuples2"), "generic/type-display declarations")

    def test_template_names(self):
        self.assertEqual(dts_family("templateLiteralTypes2"), "generic/type-display declarations")
        self.assertEqual(dts_family("templateLiteralTypes4"), "generic/type-display declarations")

    def test_string_literal_names(self):
        self.assertEqual(dts_family("stringLiteralTypesOverloads01"), "generic/type-display declarations")
        self.assertEqual(dts_family("stringLiteralTypesAndTuples01"), "generic/type-display declarations")


class TestDtsSpreadFamily(unittest.TestCase):
    def test_spread_names(self):
        self.assertEqual(dts_family("spreadDuplicate"), "spread/destructuring/variadic declarations")
        self.assertEqual(dts_family("spreadObjectOrFalsy"), "spread/destructuring/variadic declarations")

    def test_destruct_name(self):
        self.assertEqual(
            dts_family("renamingDestructuredPropertyInFunctionType"),
            "spread/destructuring/variadic declarations",
        )


class TestDtsTypeGuardFamily(unittest.TestCase):
    def test_typeguard_names(self):
        self.assertEqual(dts_family("typeGuardFunctionOfFormThis"), "type-guard declarations")
        self.assertEqual(dts_family("typeGuardOfFormThisMember(target=es2015)"), "type-guard declarations")
        self.assertEqual(dts_family("typeGuardOfFormThisMember(target=es5)"), "type-guard declarations")


class TestDtsSymlinkFamily(unittest.TestCase):
    def test_symlink_names(self):
        self.assertEqual(
            dts_family("symlinkedWorkspaceDependenciesNoDirectLinkGeneratesNonrelativeName"),
            "symlink/workspace-resolution declarations",
        )
        self.assertEqual(
            dts_family("symlinkedWorkspaceDependenciesNoDirectLinkPeerGeneratesNonrelativeName"),
            "symlink/workspace-resolution declarations",
        )


class TestDtsPrivacyFamily(unittest.TestCase):
    def test_privacy_names(self):
        self.assertEqual(
            dts_family("privacyCheckAnonymousFunctionParameter"),
            "privacy/access-modifier declarations",
        )
        self.assertEqual(
            dts_family("privacyCheckAnonymousFunctionParameter2"),
            "privacy/access-modifier declarations",
        )
        self.assertEqual(
            dts_family("privacyFunctionReturnTypeDeclFile"),
            "privacy/access-modifier declarations",
        )


class TestDtsModuleDeclExtended(unittest.TestCase):
    def test_moduledecl_name(self):
        self.assertEqual(dts_family("moduledecl(target=es2015)"), "module/declaration merging")
        self.assertEqual(dts_family("moduledecl(target=es5)"), "module/declaration merging")

    def test_existing_module_merging_still_works(self):
        self.assertEqual(dts_family("moduleAugmentation"), "module/declaration merging")


class TestDtsTsxFamily(unittest.TestCase):
    def test_tsx_classified_as_jsx_declarations(self):
        self.assertEqual(dts_family("tsxStatelessComponentDefaultProps"), "jsx/react declarations")


class TestDtsNodeModuleFamily(unittest.TestCase):
    def test_nodemodule_names(self):
        self.assertEqual(
            dts_family("nodeModulesResolveJsonModule(module=node16)"),
            "import/export/nameability",
        )
        self.assertEqual(
            dts_family("nodeModulesResolveJsonModule(module=nodenext)"),
            "import/export/nameability",
        )


class TestNegativeClassification(unittest.TestCase):
    """Verify that an unrelated test name does not trigger any of the new families."""

    _NEW_JS_FAMILIES = [
        "parser/syntax recovery emit",
        "optional-chain/nullish lowering",
        "arrow-function emit",
        "type-guard/narrowing emit",
        "unicode/reserved-word identifier emit",
        "jsdoc/js-file emit",
        "parameter-init/captured-var emit",
        "nested-function/local-scope emit",
        "type-assertion/instanceof emit",
        "tslib/helper-import emit",
        "new.target emit",
        "unused-symbol/visibility emit",
    ]

    _NEW_DTS_FAMILIES = [
        "spread/destructuring/variadic declarations",
        "type-guard declarations",
        "symlink/workspace-resolution declarations",
        "privacy/access-modifier declarations",
    ]

    def test_enum_errors_not_classified_as_new_js_family(self):
        for family in self._NEW_JS_FAMILIES:
            with self.subTest(family=family):
                self.assertNotEqual(js_family("enumErrors"), family)

    def test_enum_errors_not_classified_as_new_dts_family(self):
        for family in self._NEW_DTS_FAMILIES:
            with self.subTest(family=family):
                self.assertNotEqual(dts_family("enumErrors"), family)

    def test_js_unknown_name_falls_to_other(self):
        # A name that avoids all family needles must fall through to "other".
        # ("completely" contains "let" → block-scoping, so use xyzzy instead.)
        self.assertEqual(js_family("xyzzy_no_match_999"), "other")

    def test_dts_unknown_name_falls_to_other(self):
        self.assertEqual(dts_family("xyzzy_no_match_999"), "other")


if __name__ == "__main__":
    unittest.main()
