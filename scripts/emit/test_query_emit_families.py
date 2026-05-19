"""Tests for JS/DTS emit failure-family classifier rules in query-emit.py.

Each new or modified family needs at least two name-variant cases so that the
rule is proven general rather than tied to a single test spelling.  Negative
cases confirm that unsupported shapes still fall through to "other".
"""

import importlib.util
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
QUERY_EMIT_PATH = ROOT / "scripts" / "emit" / "query-emit.py"


def load_query_emit():
    spec = importlib.util.spec_from_file_location("query_emit", QUERY_EMIT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def make_result(name, js_error="", dts_error="", test_path="", baseline_file=""):
    return {
        "name": name,
        "testPath": test_path,
        "baselineFile": baseline_file,
        "jsError": js_error,
        "dtsError": dts_error,
        "jsStatus": "fail",
        "dtsStatus": "fail",
    }


class TestJSFamilyClassifier(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def classify(self, name, js_error=""):
        result = make_result(name, js_error=js_error)
        return self.mod.classify_failure(result, "js")

    # --- parser/recovery emit ---

    def test_parser_prefix_catches_computed_property(self):
        self.assertEqual(self.classify("parserComputedPropertyName25"), "parser/recovery emit")

    def test_parser_prefix_catches_skipped_tokens(self):
        self.assertEqual(self.classify("parserSkippedTokens13"), "parser/recovery emit")

    def test_parser_prefix_catches_real_source(self):
        self.assertEqual(self.classify("parserRealSource10"), "parser/recovery emit")

    def test_parse_bigint_needle_catches_parseBigInt(self):
        self.assertEqual(self.classify("parseBigInt"), "parser/recovery emit")

    def test_parse_error_needle_catches_parseErrorIncorrectReturnToken(self):
        self.assertEqual(self.classify("parseErrorIncorrectReturnToken"), "parser/recovery emit")

    def test_parse_invalid_needle_catches_parseInvalidNames(self):
        self.assertEqual(self.classify("parseInvalidNames"), "parser/recovery emit")

    def test_parse_assert_needle_catches_parseAssertEntriesError(self):
        self.assertEqual(self.classify("parseAssertEntriesError"), "parser/recovery emit")

    def test_skippedtoken_needle_variant(self):
        self.assertEqual(self.classify("parserSkippedTokens8"), "parser/recovery emit")

    # --- type-guard emit ---

    def test_typeguard_prefix_catches_typeGuardFunctionErrors(self):
        self.assertEqual(self.classify("typeGuardFunctionErrors"), "type-guard emit")

    def test_typeguards_prefix_catches_typeGuardsInConditionalExpression(self):
        self.assertEqual(
            self.classify("typeGuardsInConditionalExpression"), "type-guard emit"
        )

    def test_typeguards_right_operand_and(self):
        self.assertEqual(
            self.classify("typeGuardsInRightOperandOfAndAndOperator"), "type-guard emit"
        )

    def test_typeguards_right_operand_or(self):
        self.assertEqual(
            self.classify("typeGuardsInRightOperandOfOrOrOperator"), "type-guard emit"
        )

    def test_typepredicate_needle_catches_inferTypePredicates(self):
        # "typepredicate" is a substring of "infertypepredicates"
        self.assertEqual(self.classify("inferTypePredicates"), "type-guard emit")

    # --- optional-chain/nullish emit ---

    def test_chain_catches_elementAccessChain(self):
        self.assertEqual(self.classify("elementAccessChain.3"), "optional-chain/nullish emit")

    def test_chain_catches_propertyAccessChain(self):
        self.assertEqual(self.classify("propertyAccessChain.3"), "optional-chain/nullish emit")

    def test_optionalchaining_catches_optionalChainingInArrow(self):
        self.assertEqual(
            self.classify("optionalChainingInArrow(target=es5)"), "optional-chain/nullish emit"
        )

    def test_optionalchaining_catches_optionalChainingInLoop(self):
        self.assertEqual(
            self.classify("optionalChainingInLoop(target=es5)"), "optional-chain/nullish emit"
        )

    def test_chain_catches_invalidOptionalChainFromNewExpression(self):
        self.assertEqual(
            self.classify("invalidOptionalChainFromNewExpression"), "optional-chain/nullish emit"
        )

    def test_chain_catches_genericChainedCalls(self):
        self.assertEqual(self.classify("genericChainedCalls"), "optional-chain/nullish emit")

    # --- unicode/identifier-encoding emit ---

    def test_unicode_catches_invalidUnicodeEscapeSequance(self):
        self.assertEqual(
            self.classify("invalidUnicodeEscapeSequance"), "unicode/identifier-encoding emit"
        )

    def test_unicode_catches_invalidUnicodeEscapeSequance2(self):
        self.assertEqual(
            self.classify("invalidUnicodeEscapeSequance2"), "unicode/identifier-encoding emit"
        )

    def test_unicode_catches_unicodeEscapesInNames(self):
        self.assertEqual(
            self.classify("unicodeEscapesInNames02(target=es5)"), "unicode/identifier-encoding emit"
        )

    # --- reserved-word emit ---

    def test_reservedword_catches_reservedWords2(self):
        self.assertEqual(self.classify("reservedWords2"), "reserved-word emit")

    def test_reservedword_catches_reservedWords3(self):
        self.assertEqual(self.classify("reservedWords3"), "reserved-word emit")

    def test_reservedname_catches_reservedNamesInAliases(self):
        self.assertEqual(self.classify("reservedNamesInAliases"), "reserved-word emit")

    # --- js-file/plain-js emit ---

    def test_jsdeclaration_catches_jsDeclarationsNestedParams(self):
        self.assertEqual(self.classify("jsDeclarationsNestedParams"), "js-file/plain-js emit")

    def test_jsdeclaration_catches_jsDeclarationsTypeReferences4(self):
        self.assertEqual(
            self.classify("jsDeclarationsTypeReferences4(target=es5)"), "js-file/plain-js emit"
        )

    def test_jsfile_catches_jsFileCompilationEmitTrippleSlashReference(self):
        self.assertEqual(
            self.classify("jsFileCompilationEmitTrippleSlashReference"), "js-file/plain-js emit"
        )

    def test_jsfile_catches_jsFileCompilationTypeArgumentSyntaxOfCall(self):
        self.assertEqual(
            self.classify("jsFileCompilationTypeArgumentSyntaxOfCall"), "js-file/plain-js emit"
        )

    def test_plainjsgrammar_catches_plainJSGrammarErrors(self):
        self.assertEqual(self.classify("plainJSGrammarErrors"), "js-file/plain-js emit")

    # --- new-target emit ---

    def test_newtarget_catches_newTarget_es5(self):
        self.assertEqual(self.classify("newTarget.es5(target=es5)"), "new-target emit")

    def test_newtarget_catches_invalidNewTarget_es5(self):
        self.assertEqual(self.classify("invalidNewTarget.es5(target=es5)"), "new-target emit")

    # --- tslib/helper emit ---

    def test_tslib_catches_tslibMissingHelper(self):
        self.assertEqual(self.classify("tslibMissingHelper"), "tslib/helper emit")

    def test_tslib_catches_tslibMultipleMissingHelper(self):
        self.assertEqual(self.classify("tslibMultipleMissingHelper"), "tslib/helper emit")

    # --- jsdoc-type emit ---

    def test_jsdoc_catches_expressionWithJSDocTypeArguments(self):
        self.assertEqual(self.classify("expressionWithJSDocTypeArguments"), "jsdoc-type emit")

    # --- tsx extension added to jsx/react family ---

    def test_tsx_catches_tsxStatelessComponentDefaultProps(self):
        self.assertEqual(self.classify("tsxStatelessComponentDefaultProps"), "jsx/react emit")

    def test_tsx_catches_tsxUnionMemberChecksFilterDataProps(self):
        self.assertEqual(self.classify("tsxUnionMemberChecksFilterDataProps"), "jsx/react emit")

    # --- Rule ordering: existing rules take priority over new ones ---

    def test_existing_async_rule_not_overridden_by_parser(self):
        # Existing rules must fire before the new extended families (order check).
        result = self.classify("asyncGeneratorParameterEvaluation(target=es2015)")
        self.assertEqual(result, "async/await/generator lowering")

    def test_existing_class_rule_not_overridden_by_chain(self):
        result = self.classify("classStaticBlock18(target=es5)")
        self.assertEqual(result, "class/private/accessor/decorator lowering")

    # --- "other" as the final fallback ---

    def test_truly_unclassified_falls_to_other(self):
        # Avoid names that contain existing needles as substrings (e.g. "let" in "completely").
        self.assertEqual(self.classify("unknownXyzAbcDef99"), "other")

    def test_giant_test_still_other(self):
        self.assertEqual(self.classify("giant"), "other")


class TestDTSFamilyClassifier(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_query_emit()

    def classify(self, name, dts_error=""):
        result = make_result(name, dts_error=dts_error)
        return self.mod.classify_failure(result, "dts")

    # --- jsdoc/javascript declarations: new jsfile needle ---

    def test_jsfile_catches_jsFileAlternativeUseOfOverloadTag(self):
        self.assertEqual(
            self.classify("jsFileAlternativeUseOfOverloadTag"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileCompilationDuplicateVariable(self):
        self.assertEqual(
            self.classify("jsFileCompilationDuplicateVariable"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileFunctionOverloads(self):
        self.assertEqual(
            self.classify("jsFileFunctionOverloads"), "jsdoc/javascript declarations"
        )

    def test_jsfile_catches_jsFileMethodOverloads2(self):
        self.assertEqual(
            self.classify("jsFileMethodOverloads2"), "jsdoc/javascript declarations"
        )

    # --- module/declaration merging: new symlink/moduledecl/nodemodule needles ---

    def test_symlink_catches_symlinkedWorkspaceDependencies(self):
        self.assertEqual(
            self.classify("symlinkedWorkspaceDependenciesNoDirectLinkGeneratesNonrelativeName"),
            "module/declaration merging",
        )

    def test_symlink_catches_symlinkedWorkspaceOptional(self):
        self.assertEqual(
            self.classify(
                "symlinkedWorkspaceDependenciesNoDirectLinkOptionalGeneratesNonrelativeName"
            ),
            "module/declaration merging",
        )

    def test_moduledecl_catches_moduledecl_es2015(self):
        self.assertEqual(
            self.classify("moduledecl(target=es2015)"), "module/declaration merging"
        )

    def test_moduledecl_catches_moduledecl_es5(self):
        self.assertEqual(
            self.classify("moduledecl(target=es5)"), "module/declaration merging"
        )

    def test_nodemodule_catches_nodeModulesResolveJsonModule(self):
        self.assertEqual(
            self.classify("nodeModulesResolveJsonModule(module=node16)"),
            "module/declaration merging",
        )

    def test_nodemodule_catches_nodeModulesResolveJsonModule_nodenext(self):
        self.assertEqual(
            self.classify("nodeModulesResolveJsonModule(module=nodenext)"),
            "module/declaration merging",
        )

    # --- class/private/accessor declarations: new privacy needle ---

    def test_privacy_catches_privacyCheckAnonymousFunctionParameter(self):
        self.assertEqual(
            self.classify("privacyCheckAnonymousFunctionParameter"),
            "class/private/accessor declarations",
        )

    def test_privacy_catches_privacyCheckAnonymousFunctionParameter2(self):
        self.assertEqual(
            self.classify("privacyCheckAnonymousFunctionParameter2"),
            "class/private/accessor declarations",
        )

    def test_privacy_catches_privacyFunctionReturnTypeDeclFile(self):
        self.assertEqual(
            self.classify("privacyFunctionReturnTypeDeclFile"),
            "class/private/accessor declarations",
        )

    # --- generic/type-display declarations: extended needles ---

    def test_template_catches_templateLiteralTypes2(self):
        self.assertEqual(
            self.classify("templateLiteralTypes2"), "generic/type-display declarations"
        )

    def test_template_catches_templateLiteralTypes4(self):
        self.assertEqual(
            self.classify("templateLiteralTypes4"), "generic/type-display declarations"
        )

    def test_variadic_catches_variadicTuples1(self):
        self.assertEqual(
            self.classify("variadicTuples1"), "generic/type-display declarations"
        )

    def test_variadic_catches_variadicTuples2(self):
        self.assertEqual(
            self.classify("variadicTuples2"), "generic/type-display declarations"
        )

    def test_tuple_catches_restTuplesFromContextualTypes(self):
        self.assertEqual(
            self.classify("restTuplesFromContextualTypes"), "generic/type-display declarations"
        )

    def test_tuple_catches_namedTupleMembers(self):
        self.assertEqual(
            self.classify("namedTupleMembers"), "generic/type-display declarations"
        )

    def test_stringliteral_catches_stringLiteralTypesOverloads01(self):
        self.assertEqual(
            self.classify("stringLiteralTypesOverloads01"), "generic/type-display declarations"
        )

    def test_stringliteral_catches_stringLiteralTypesAndTuples01(self):
        self.assertEqual(
            self.classify("stringLiteralTypesAndTuples01"), "generic/type-display declarations"
        )

    def test_spread_catches_spreadDuplicate(self):
        self.assertEqual(self.classify("spreadDuplicate"), "generic/type-display declarations")

    def test_spread_catches_spreadObjectOrFalsy(self):
        self.assertEqual(
            self.classify("spreadObjectOrFalsy"), "generic/type-display declarations"
        )

    def test_never_catches_neverType(self):
        self.assertEqual(self.classify("neverType"), "generic/type-display declarations")

    def test_never_catches_silentNeverPropagation(self):
        self.assertEqual(
            self.classify("silentNeverPropagation"), "generic/type-display declarations"
        )

    def test_noimplicit_catches_noImplicitThisBigThis(self):
        self.assertEqual(
            self.classify("noImplicitThisBigThis"), "generic/type-display declarations"
        )

    # --- type-guard declarations (new family) ---

    def test_typeguard_catches_typeGuardFunctionOfFormThis(self):
        self.assertEqual(
            self.classify("typeGuardFunctionOfFormThis"), "type-guard declarations"
        )

    def test_typeguard_catches_typeGuardOfFormThisMember(self):
        self.assertEqual(
            self.classify("typeGuardOfFormThisMember(target=es2015)"), "type-guard declarations"
        )

    def test_typeguard_catches_typeGuardOfFormThisMember_es5(self):
        self.assertEqual(
            self.classify("typeGuardOfFormThisMember(target=es5)"), "type-guard declarations"
        )

    # --- unique-symbol declarations (new family) ---

    def test_uniquesymbol_catches_uniqueSymbolsDeclarations(self):
        self.assertEqual(
            self.classify("uniqueSymbolsDeclarations"), "unique-symbol declarations"
        )

    # --- "other" as the final fallback ---

    def test_giant_still_other_in_dts(self):
        self.assertEqual(self.classify("giant"), "other")

    def test_truly_unclassified_falls_to_other_dts(self):
        self.assertEqual(self.classify("unknownXyzAbcDef99"), "other")


if __name__ == "__main__":
    unittest.main()
