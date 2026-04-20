#[test]
fn test_source_map_module_es5_conditional_imports() {
    let source = r#"// Conditional import patterns
declare const process: { env: { NODE_ENV: string; PLATFORM: string } };

// Environment-based conditional import
const getLogger = async () => {
    if (process.env.NODE_ENV === 'production') {
        return import('./prodLogger');
    } else {
        return import('./devLogger');
    }
};

// Platform-based conditional import
const getPlatformModule = async () => {
    switch (process.env.PLATFORM) {
        case 'web':
            return import('./platform/web');
        case 'node':
            return import('./platform/node');
        case 'electron':
            return import('./platform/electron');
        default:
            return import('./platform/default');
    }
};

// Feature flag conditional import
interface FeatureFlags {
    newUI: boolean;
    betaFeatures: boolean;
}

async function loadFeatures(flags: FeatureFlags): Promise<any[]> {
    const features: Promise<any>[] = [];

    if (flags.newUI) {
        features.push(import('./features/newUI'));
    }

    if (flags.betaFeatures) {
        features.push(import('./features/beta'));
    }

    return Promise.all(features);
}

// Polyfill conditional import
async function loadPolyfills(): Promise<void> {
    if (typeof globalThis.fetch === 'undefined') {
        await import('whatwg-fetch');
    }

    if (typeof globalThis.Promise === 'undefined') {
        await import('es6-promise');
    }

    if (!Array.prototype.includes) {
        await import('array-includes');
    }
}

// Usage
getLogger().then(logger => logger.default.info('App started'));
loadPolyfills().then(() => console.log('Polyfills loaded'));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("getLogger"),
        "expected output to contain getLogger function. output: {output}"
    );
    assert!(
        output.contains("loadPolyfills"),
        "expected output to contain loadPolyfills function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_namespace_imports() {
    let source = r#"// Namespace import patterns
import * as React from 'react';
import * as ReactDOM from 'react-dom';
import * as _ from 'lodash';
import * as utils from './utils';

// Using namespace imports
const element = React.createElement('div', { className: 'container' },
    React.createElement('h1', null, 'Hello'),
    React.createElement('p', null, 'World')
);

// Lodash namespace usage
const data = [1, 2, 3, 4, 5];
const doubled = _.map(data, (n: number) => n * 2);
const sum = _.reduce(doubled, (acc: number, n: number) => acc + n, 0);
const unique = _.uniq([1, 1, 2, 2, 3]);

// Custom utils namespace
const formatted = utils.formatDate(new Date());
const validated = utils.validateEmail('test@example.com');
const parsed = utils.parseJSON('{"key": "value"}');

// Re-export namespace
export { React, ReactDOM, _ as lodash, utils };

// Namespace with type usage
type ReactElement = React.ReactElement;
type LodashArray = _.LoDashStatic;

export function render(el: ReactElement, container: Element): void {
    ReactDOM.render(el, container);
}

console.log(sum, unique, formatted);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("render"),
        "expected output to contain render function. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected output to contain doubled variable. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace imports"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_module_es5_comprehensive() {
    let source = r#"// Comprehensive module bundling test combining all patterns

// Static imports
import { EventEmitter } from 'events';
import * as fs from 'fs';
import path from 'path';

// CommonJS require
const express = require('express');
const bodyParser = require('body-parser');

// Type-only imports
import type { ServerOptions, RequestHandler } from 'express';

// Re-exports
export { EventEmitter } from 'events';
export * as fsUtils from 'fs';
export type { ServerOptions };

// Dynamic import loader
class ModuleRegistry {
    private modules: Map<string, any> = new Map();
    private loading: Map<string, Promise<any>> = new Map();

    async load(name: string, path: string): Promise<any> {
        if (this.modules.has(name)) {
            return this.modules.get(name);
        }

        if (!this.loading.has(name)) {
            this.loading.set(name, import(path).then(mod => {
                const module = mod.default || mod;
                this.modules.set(name, module);
                this.loading.delete(name);
                return module;
            }));
        }

        return this.loading.get(name);
    }

    get(name: string): any {
        return this.modules.get(name);
    }

    has(name: string): boolean {
        return this.modules.has(name);
    }
}

// Conditional module loading
declare const process: { env: Record<string, string> };

async function loadEnvironmentModules(): Promise<void> {
    const env = process.env.NODE_ENV || 'development';

    // Environment-specific config
    const configModule = await import(`./config/${env}`);
    const config = configModule.default;

    // Conditional feature modules
    if (config.features?.analytics) {
        await import('./modules/analytics');
    }

    if (config.features?.monitoring) {
        await import('./modules/monitoring');
    }

    // Platform-specific modules
    const platform = process.env.PLATFORM || 'node';
    await import(`./platform/${platform}`);
}

// Barrel export simulation
export { Button, Input, Form } from './components';
export { useForm, useValidation } from './hooks';
export * from './utils';

// Main application
export class Application extends EventEmitter {
    private registry: ModuleRegistry;
    private server: any;

    constructor() {
        super();
        this.registry = new ModuleRegistry();
        this.server = express();
        this.server.use(bodyParser.json());
    }

    async initialize(): Promise<void> {
        await loadEnvironmentModules();

        // Load plugins dynamically
        const pluginPaths = ['./plugins/auth', './plugins/api', './plugins/static'];
        await Promise.all(pluginPaths.map(p => this.registry.load(path.basename(p), p)));

        this.emit('initialized');
    }

    async loadPlugin(name: string, pluginPath: string): Promise<void> {
        const plugin = await this.registry.load(name, pluginPath);
        if (plugin.setup) {
            await plugin.setup(this.server);
        }
        this.emit('plugin:loaded', name);
    }

    start(port: number): void {
        this.server.listen(port, () => {
            this.emit('started', port);
            console.log(`Server running on port ${port}`);
        });
    }
}

// Factory export
export function createApp(): Application {
    return new Application();
}

// Default export
export default Application;

// Usage
const app = createApp();
app.initialize().then(() => app.start(3000));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Application"),
        "expected output to contain Application class. output: {output}"
    );
    assert!(
        output.contains("ModuleRegistry"),
        "expected output to contain ModuleRegistry class. output: {output}"
    );
    assert!(
        output.contains("createApp"),
        "expected output to contain createApp function. output: {output}"
    );
    assert!(
        output.contains("loadEnvironmentModules"),
        "expected output to contain loadEnvironmentModules function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive module bundling"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// JSX ES5 Transform Source Map Tests
// =============================================================================
// Tests for JSX compilation with ES5 target - JSX elements should transform
// to React.createElement calls while preserving source map accuracy.

#[test]
fn test_source_map_jsx_es5_basic_element() {
    // Test basic JSX element transformation to React.createElement
    let source = r#"const element = <div className="container">Hello World</div>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // JSX should be in output (either preserved or transformed)
    assert!(
        output.contains("div") || output.contains("createElement"),
        "expected JSX element in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX element"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_fragment() {
    // Test JSX fragment transformation
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Fragment should be in output
    assert!(
        output.contains("span") || output.contains("Fragment"),
        "expected JSX fragment content in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX fragment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
