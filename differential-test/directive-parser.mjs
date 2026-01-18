/**
 * Directive Parser - Parses TypeScript test directives from source files
 *
 * This module implements parsing for @ directives commonly used in TypeScript
 * conformance tests. The parsing behavior matches how TSC's test runner handles
 * these directives.
 *
 * Supported directives:
 *
 * Type Checking Options:
 * - @strict: boolean - Enable all strict type checking options
 * - @noImplicitAny: boolean - Raise error on implied 'any' type
 * - @strictNullChecks: boolean - Enable strict null checking
 * - @noImplicitReturns: boolean - Error on missing return statements
 * - @noImplicitThis: boolean - Error on implicit this types
 * - @strictFunctionTypes: boolean - Enable strict function type checking
 * - @strictPropertyInitialization: boolean - Ensure class properties are initialized
 * - @strictBindCallApply: boolean - Enable strict bind, call, and apply checks
 * - @alwaysStrict: boolean - Parse in strict mode and emit "use strict"
 * - @noUnusedLocals: boolean - Report errors on unused locals
 * - @noUnusedParameters: boolean - Report errors on unused parameters
 * - @exactOptionalPropertyTypes: boolean - Interpret optional property types as written
 * - @noUncheckedIndexedAccess: boolean - Include undefined in index signature results
 * - @noPropertyAccessFromIndexSignature: boolean - Require indexed access for types declared with index signature
 * - @noFallthroughCasesInSwitch: boolean - Ensure all switch cases include return or break
 * - @allowUnreachableCode: boolean - Disable error reporting for unreachable code
 * - @allowUnusedLabels: boolean - Disable error reporting for unused labels
 *
 * Emit Options:
 * - @target: string - ECMAScript target version (es5, es2015, es2020, esnext, etc.)
 * - @module: string - Module system (commonjs, esnext, es2015, es2020, es2022, node16, nodenext, preserve, none)
 * - @moduleResolution: string - Module resolution strategy (node, classic, bundler, node16, nodenext)
 * - @lib: string - Comma-separated list of lib files (es2020, dom, esnext, etc.)
 * - @jsx: string - JSX handling (preserve, react, react-native, react-jsx, react-jsxdev)
 * - @jsxFactory: string - Specify the JSX factory function to use
 * - @jsxFragmentFactory: string - Specify the JSX Fragment reference
 * - @jsxImportSource: string - Specify module specifier for JSX factory functions
 * - @declaration: boolean - Generate .d.ts declaration files
 * - @declarationMap: boolean - Generate sourcemaps for .d.ts files
 * - @sourceMap: boolean - Generate source map files
 * - @inlineSourceMap: boolean - Include source maps in emitted JavaScript
 * - @inlineSources: boolean - Include source code in sourcemaps
 * - @noEmit: boolean - Do not emit output
 * - @removeComments: boolean - Remove comments from output
 * - @importHelpers: boolean - Import emit helpers from tslib
 * - @downlevelIteration: boolean - Enable downleveling for iteration
 * - @emitDecoratorMetadata: boolean - Emit design-type metadata for decorated declarations
 * - @experimentalDecorators: boolean - Enable experimental decorator support
 * - @useDefineForClassFields: boolean - Emit ECMAScript standard class fields
 *
 * Module Options:
 * - @esModuleInterop: boolean - Enable ES module interoperability
 * - @allowSyntheticDefaultImports: boolean - Allow default imports from modules with no default export
 * - @verbatimModuleSyntax: boolean - Preserve module syntax exactly as written
 * - @isolatedModules: boolean - Ensure each file can be safely transpiled without relying on other imports
 * - @allowUmdGlobalAccess: boolean - Allow accessing UMD globals from modules
 * - @preserveSymlinks: boolean - Do not resolve symlinks to their real path
 * - @resolveJsonModule: boolean - Enable importing .json files
 * - @allowArbitraryExtensions: boolean - Enable importing files with any extension
 * - @customConditions: string - Comma-separated list of custom conditions for --conditions
 *
 * Type Acquisition:
 * - @noLib: boolean - Disable including default lib file
 * - @skipLibCheck: boolean - Skip type checking of declaration files
 * - @skipDefaultLibCheck: boolean - Skip type checking of default library declaration files
 *
 * JavaScript Support:
 * - @allowJs: boolean - Allow JavaScript files to be compiled
 * - @checkJs: boolean - Report errors in JavaScript files
 * - @maxNodeModuleJsDepth: number - Maximum depth for searching node_modules for JavaScript files
 *
 * Editor Support:
 * - @disableSizeLimit: boolean - Disable JavaScript file size limit
 * - @disableSolutionSearching: boolean - Opt out of multi-project reference checking
 * - @disableReferencedProjectLoad: boolean - Reduce projects loaded automatically
 *
 * Output Formatting:
 * - @noErrorTruncation: boolean - Disable truncating error messages
 * - @preserveConstEnums: boolean - Preserve const enum declarations
 * - @newLine: string - Set the newline character (crlf or lf)
 * - @charset: string - Set character encoding of input files
 *
 * Paths:
 * - @baseUrl: string - Base directory for resolving non-relative module names
 * - @rootDir: string - Root directory of input files
 * - @rootDirs: string - Comma-separated list of root directories
 * - @outDir: string - Output directory
 * - @outFile: string - Concatenate and emit output to single file
 * - @declarationDir: string - Output directory for declaration files
 *
 * Multi-file Tests:
 * - @filename: string - Marks file boundaries in multi-file tests (special directive)
 */

/**
 * Parse TypeScript test directives from source code.
 *
 * @param {string} code - Source code potentially containing test directives
 * @returns {{
 *   options: Record<string, any>,
 *   isMultiFile: boolean,
 *   cleanCode: string,
 *   files: Array<{name: string, content: string}>
 * }} Parsed directives and cleaned code
 */
export function parseTestDirectives(code) {
  const lines = code.split('\n');
  const options = {};
  let isMultiFile = false;
  const cleanLines = [];
  const files = [];

  let currentFileName = null;
  let currentFileLines = [];

  for (const line of lines) {
    const trimmed = line.trim();

    // Check for @filename directive (multi-file test)
    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);
    if (filenameMatch) {
      isMultiFile = true;
      // Save previous file if any
      if (currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
      }
      currentFileName = filenameMatch[1].trim();
      currentFileLines = [];
      continue;
    }

    // Parse compiler options like // @strict: true or // @target: es2020
    // Also support directives without colons like // @strict (treated as true)
    const matchWithValue = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/);
    const matchWithoutValue = trimmed.match(/^\/\/\s*@(\w+)\s*$/);

    if (matchWithValue) {
      const [, key, rawValue] = matchWithValue;
      const normalizedKey = key.toLowerCase();
      const value = rawValue.trim();

      // Handle special directives that accumulate values (like @lib)
      if (normalizedKey === 'lib' || normalizedKey === 'rootdirs' || normalizedKey === 'customconditions') {
        if (!options[normalizedKey]) {
          options[normalizedKey] = [];
        }
        const items = value.split(',').map(item => item.trim()).filter(item => item.length > 0);
        options[normalizedKey].push(...items);
      }
      // Parse boolean values
      else if (value.toLowerCase() === 'true') {
        options[normalizedKey] = true;
      } else if (value.toLowerCase() === 'false') {
        options[normalizedKey] = false;
      }
      // Parse numeric values
      else if (!isNaN(Number(value))) {
        options[normalizedKey] = Number(value);
      }
      // String value
      else {
        options[normalizedKey] = value;
      }
      continue;
    } else if (matchWithoutValue) {
      // Directives without values are treated as boolean true
      const [, key] = matchWithoutValue;
      options[key.toLowerCase()] = true;
      continue;
    }

    // Collect lines for multi-file tests or clean code
    if (isMultiFile && currentFileName) {
      currentFileLines.push(line);
    } else {
      cleanLines.push(line);
    }
  }

  // Save the last file for multi-file tests
  if (isMultiFile && currentFileName) {
    files.push({ name: currentFileName, content: currentFileLines.join('\n') });
  }

  return {
    options,
    isMultiFile,
    cleanCode: cleanLines.join('\n'),
    files,
  };
}

/**
 * Map parsed directive options to TypeScript compiler options.
 * This function takes the raw parsed options (with lowercase keys)
 * and transforms them to proper TypeScript CompilerOptions.
 *
 * @param {Record<string, any>} parsedOptions - Options from parseTestDirectives
 * @param {object} ts - TypeScript module (required for enum values)
 * @returns {object} TypeScript CompilerOptions
 */
export function mapToCompilerOptions(parsedOptions, ts) {
  const compilerOptions = {};

  // Target mapping
  if (parsedOptions.target !== undefined) {
    const targetMap = {
      'es3': ts.ScriptTarget.ES3,
      'es5': ts.ScriptTarget.ES5,
      'es6': ts.ScriptTarget.ES2015,
      'es2015': ts.ScriptTarget.ES2015,
      'es2016': ts.ScriptTarget.ES2016,
      'es2017': ts.ScriptTarget.ES2017,
      'es2018': ts.ScriptTarget.ES2018,
      'es2019': ts.ScriptTarget.ES2019,
      'es2020': ts.ScriptTarget.ES2020,
      'es2021': ts.ScriptTarget.ES2021,
      'es2022': ts.ScriptTarget.ES2022,
      'es2023': ts.ScriptTarget.ES2023,
      'esnext': ts.ScriptTarget.ESNext,
    };
    const target = String(parsedOptions.target).toLowerCase();
    compilerOptions.target = targetMap[target] ?? ts.ScriptTarget.ES2020;
  }

  // Module mapping
  if (parsedOptions.module !== undefined) {
    const moduleMap = {
      'none': ts.ModuleKind.None,
      'commonjs': ts.ModuleKind.CommonJS,
      'amd': ts.ModuleKind.AMD,
      'umd': ts.ModuleKind.UMD,
      'system': ts.ModuleKind.System,
      'es6': ts.ModuleKind.ES2015,
      'es2015': ts.ModuleKind.ES2015,
      'es2020': ts.ModuleKind.ES2020,
      'es2022': ts.ModuleKind.ES2022,
      'esnext': ts.ModuleKind.ESNext,
      'node16': ts.ModuleKind.Node16,
      'nodenext': ts.ModuleKind.NodeNext,
      'preserve': ts.ModuleKind.Preserve,
    };
    const module = String(parsedOptions.module).toLowerCase();
    if (moduleMap[module] !== undefined) {
      compilerOptions.module = moduleMap[module];
    }
  }

  // Module resolution mapping
  if (parsedOptions.moduleresolution !== undefined) {
    const resolutionMap = {
      'classic': ts.ModuleResolutionKind.Classic,
      'node': ts.ModuleResolutionKind.Node10 ?? ts.ModuleResolutionKind.NodeJs,
      'node10': ts.ModuleResolutionKind.Node10 ?? ts.ModuleResolutionKind.NodeJs,
      'node16': ts.ModuleResolutionKind.Node16,
      'nodenext': ts.ModuleResolutionKind.NodeNext,
      'bundler': ts.ModuleResolutionKind.Bundler,
    };
    const resolution = String(parsedOptions.moduleresolution).toLowerCase();
    if (resolutionMap[resolution] !== undefined) {
      compilerOptions.moduleResolution = resolutionMap[resolution];
    }
  }

  // JSX mapping
  if (parsedOptions.jsx !== undefined) {
    const jsxMap = {
      'preserve': ts.JsxEmit.Preserve,
      'react': ts.JsxEmit.React,
      'react-native': ts.JsxEmit.ReactNative,
      'react-jsx': ts.JsxEmit.ReactJSX,
      'react-jsxdev': ts.JsxEmit.ReactJSXDev,
    };
    const jsx = String(parsedOptions.jsx).toLowerCase();
    if (jsxMap[jsx] !== undefined) {
      compilerOptions.jsx = jsxMap[jsx];
    }
  }

  // NewLine mapping
  if (parsedOptions.newline !== undefined) {
    const newLineMap = {
      'crlf': ts.NewLineKind.CarriageReturnLineFeed,
      'lf': ts.NewLineKind.LineFeed,
    };
    const newLine = String(parsedOptions.newline).toLowerCase();
    if (newLineMap[newLine] !== undefined) {
      compilerOptions.newLine = newLineMap[newLine];
    }
  }

  // Boolean options - direct mapping
  const booleanOptions = [
    // Strict mode options
    ['strict', 'strict'],
    ['noimplicitany', 'noImplicitAny'],
    ['strictnullchecks', 'strictNullChecks'],
    ['noimplicitreturns', 'noImplicitReturns'],
    ['noimplicitthis', 'noImplicitThis'],
    ['strictfunctiontypes', 'strictFunctionTypes'],
    ['strictpropertyinitialization', 'strictPropertyInitialization'],
    ['strictbindcallapply', 'strictBindCallApply'],
    ['alwaysstrict', 'alwaysStrict'],
    ['nounusedlocals', 'noUnusedLocals'],
    ['nounusedparameters', 'noUnusedParameters'],
    ['exactoptionalpropertytypes', 'exactOptionalPropertyTypes'],
    ['nouncheckedindexedaccess', 'noUncheckedIndexedAccess'],
    ['nopropertyaccessfromindexsignature', 'noPropertyAccessFromIndexSignature'],
    ['nofallthroughcasesinswitch', 'noFallthroughCasesInSwitch'],
    ['allowunreachablecode', 'allowUnreachableCode'],
    ['allowunusedlabels', 'allowUnusedLabels'],

    // Emit options
    ['declaration', 'declaration'],
    ['declarationmap', 'declarationMap'],
    ['sourcemap', 'sourceMap'],
    ['inlinesourcemap', 'inlineSourceMap'],
    ['inlinesources', 'inlineSources'],
    ['noemit', 'noEmit'],
    ['removecomments', 'removeComments'],
    ['importhelpers', 'importHelpers'],
    ['downleveliteration', 'downlevelIteration'],
    ['emitdecoratormetadata', 'emitDecoratorMetadata'],
    ['experimentaldecorators', 'experimentalDecorators'],
    ['usedefineforclassfields', 'useDefineForClassFields'],

    // Module options
    ['esmoduleinterop', 'esModuleInterop'],
    ['allowsyntheticdefaultimports', 'allowSyntheticDefaultImports'],
    ['verbatimmodulesyntax', 'verbatimModuleSyntax'],
    ['isolatedmodules', 'isolatedModules'],
    ['allowumdglobalaccess', 'allowUmdGlobalAccess'],
    ['preservesymlinks', 'preserveSymlinks'],
    ['resolvejsonmodule', 'resolveJsonModule'],
    ['allowarbitraryextensions', 'allowArbitraryExtensions'],

    // Type acquisition
    ['nolib', 'noLib'],
    ['skiplibcheck', 'skipLibCheck'],
    ['skipdefaultlibcheck', 'skipDefaultLibCheck'],

    // JavaScript support
    ['allowjs', 'allowJs'],
    ['checkjs', 'checkJs'],

    // Editor support
    ['disablesizelimit', 'disableSizeLimit'],
    ['disablesolutionsearching', 'disableSolutionSearching'],
    ['disablereferencedprojectload', 'disableReferencedProjectLoad'],

    // Output formatting
    ['noerrortruncation', 'noErrorTruncation'],
    ['preserveconstenums', 'preserveConstEnums'],
  ];

  for (const [parsedKey, compilerKey] of booleanOptions) {
    if (parsedOptions[parsedKey] !== undefined) {
      compilerOptions[compilerKey] = parsedOptions[parsedKey];
    }
  }

  // String options - direct mapping
  const stringOptions = [
    ['jsxfactory', 'jsxFactory'],
    ['jsxfragmentfactory', 'jsxFragmentFactory'],
    ['jsximportsource', 'jsxImportSource'],
    ['baseurl', 'baseUrl'],
    ['rootdir', 'rootDir'],
    ['outdir', 'outDir'],
    ['outfile', 'outFile'],
    ['declarationdir', 'declarationDir'],
    ['charset', 'charset'],
  ];

  for (const [parsedKey, compilerKey] of stringOptions) {
    if (parsedOptions[parsedKey] !== undefined) {
      compilerOptions[compilerKey] = parsedOptions[parsedKey];
    }
  }

  // Numeric options
  if (parsedOptions.maxnodemodulejsdepth !== undefined) {
    compilerOptions.maxNodeModuleJsDepth = parsedOptions.maxnodemodulejsdepth;
  }

  // Array options (lib is handled specially when loading lib files)
  if (parsedOptions.lib !== undefined) {
    compilerOptions.lib = parsedOptions.lib;
  }

  if (parsedOptions.rootdirs !== undefined) {
    compilerOptions.rootDirs = parsedOptions.rootdirs;
  }

  return compilerOptions;
}

/**
 * Map parsed directive options to a JSON-serializable format for WASM compiler.
 * This function creates an options object that can be serialized to JSON
 * and passed to the WASM compiler's setCompilerOptions method.
 *
 * @param {Record<string, any>} parsedOptions - Options from parseTestDirectives
 * @returns {object} JSON-serializable compiler options
 */
export function mapToWasmCompilerOptions(parsedOptions) {
  const compilerOptions = {};

  // Target (as string)
  if (parsedOptions.target !== undefined) {
    compilerOptions.target = String(parsedOptions.target).toLowerCase();
  }

  // Module (as string)
  if (parsedOptions.module !== undefined) {
    compilerOptions.module = String(parsedOptions.module).toLowerCase();
  }

  // Module resolution (as string)
  if (parsedOptions.moduleresolution !== undefined) {
    compilerOptions.moduleResolution = String(parsedOptions.moduleresolution).toLowerCase();
  }

  // JSX (as string)
  if (parsedOptions.jsx !== undefined) {
    compilerOptions.jsx = String(parsedOptions.jsx).toLowerCase();
  }

  // Boolean options
  const booleanOptions = [
    ['strict', 'strict'],
    ['noimplicitany', 'noImplicitAny'],
    ['strictnullchecks', 'strictNullChecks'],
    ['noimplicitreturns', 'noImplicitReturns'],
    ['noimplicitthis', 'noImplicitThis'],
    ['strictfunctiontypes', 'strictFunctionTypes'],
    ['strictpropertyinitialization', 'strictPropertyInitialization'],
    ['strictbindcallapply', 'strictBindCallApply'],
    ['alwaysstrict', 'alwaysStrict'],
    ['nounusedlocals', 'noUnusedLocals'],
    ['nounusedparameters', 'noUnusedParameters'],
    ['exactoptionalpropertytypes', 'exactOptionalPropertyTypes'],
    ['nouncheckedindexedaccess', 'noUncheckedIndexedAccess'],
    ['nopropertyaccessfromindexsignature', 'noPropertyAccessFromIndexSignature'],
    ['nofallthroughcasesinswitch', 'noFallthroughCasesInSwitch'],
    ['allowunreachablecode', 'allowUnreachableCode'],
    ['allowunusedlabels', 'allowUnusedLabels'],
    ['declaration', 'declaration'],
    ['declarationmap', 'declarationMap'],
    ['sourcemap', 'sourceMap'],
    ['inlinesourcemap', 'inlineSourceMap'],
    ['inlinesources', 'inlineSources'],
    ['noemit', 'noEmit'],
    ['removecomments', 'removeComments'],
    ['importhelpers', 'importHelpers'],
    ['downleveliteration', 'downlevelIteration'],
    ['emitdecoratormetadata', 'emitDecoratorMetadata'],
    ['experimentaldecorators', 'experimentalDecorators'],
    ['usedefineforclassfields', 'useDefineForClassFields'],
    ['esmoduleinterop', 'esModuleInterop'],
    ['allowsyntheticdefaultimports', 'allowSyntheticDefaultImports'],
    ['verbatimmodulesyntax', 'verbatimModuleSyntax'],
    ['isolatedmodules', 'isolatedModules'],
    ['allowumdglobalaccess', 'allowUmdGlobalAccess'],
    ['preservesymlinks', 'preserveSymlinks'],
    ['resolvejsonmodule', 'resolveJsonModule'],
    ['allowarbitraryextensions', 'allowArbitraryExtensions'],
    ['nolib', 'noLib'],
    ['skiplibcheck', 'skipLibCheck'],
    ['skipdefaultlibcheck', 'skipDefaultLibCheck'],
    ['allowjs', 'allowJs'],
    ['checkjs', 'checkJs'],
    ['noerrortruncation', 'noErrorTruncation'],
    ['preserveconstenums', 'preserveConstEnums'],
  ];

  for (const [parsedKey, compilerKey] of booleanOptions) {
    if (parsedOptions[parsedKey] !== undefined) {
      compilerOptions[compilerKey] = parsedOptions[parsedKey];
    }
  }

  // String options
  const stringOptions = [
    ['jsxfactory', 'jsxFactory'],
    ['jsxfragmentfactory', 'jsxFragmentFactory'],
    ['jsximportsource', 'jsxImportSource'],
    ['baseurl', 'baseUrl'],
    ['rootdir', 'rootDir'],
    ['outdir', 'outDir'],
    ['outfile', 'outFile'],
    ['declarationdir', 'declarationDir'],
    ['charset', 'charset'],
    ['newline', 'newLine'],
  ];

  for (const [parsedKey, compilerKey] of stringOptions) {
    if (parsedOptions[parsedKey] !== undefined) {
      compilerOptions[compilerKey] = parsedOptions[parsedKey];
    }
  }

  // Array options
  if (parsedOptions.lib !== undefined) {
    compilerOptions.lib = parsedOptions.lib;
  }

  if (parsedOptions.rootdirs !== undefined) {
    compilerOptions.rootDirs = parsedOptions.rootdirs;
  }

  // Numeric options
  if (parsedOptions.maxnodemodulejsdepth !== undefined) {
    compilerOptions.maxNodeModuleJsDepth = parsedOptions.maxnodemodulejsdepth;
  }

  return compilerOptions;
}

export default {
  parseTestDirectives,
  mapToCompilerOptions,
  mapToWasmCompilerOptions,
};
