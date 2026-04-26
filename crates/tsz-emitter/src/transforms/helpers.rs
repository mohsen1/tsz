//! TypeScript Runtime Helpers
//!
//! These are the helper functions that TypeScript injects into output
//! when downleveling ES2015+ features to ES5.

/// Helper code for __extends (class inheritance)
pub const EXTENDS_HELPER: &str = r#"var __extends = (this && this.__extends) || (function () {
    var extendStatics = function (d, b) {
        extendStatics = Object.setPrototypeOf ||
            ({ __proto__: [] } instanceof Array && function (d, b) { d.__proto__ = b; }) ||
            function (d, b) { for (var p in b) if (Object.prototype.hasOwnProperty.call(b, p)) d[p] = b[p]; };
        return extendStatics(d, b);
    };
    return function (d, b) {
        if (typeof b !== "function" && b !== null)
            throw new TypeError("Class extends value " + String(b) + " is not a constructor or null");
        extendStatics(d, b);
        function __() { this.constructor = d; }
        d.prototype = b === null ? Object.create(b) : (__.prototype = b.prototype, new __());
    };
})();"#;

/// Helper code for __assign (object spread)
pub const ASSIGN_HELPER: &str = r#"var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};"#;

/// Helper code for __rest (destructuring rest)
pub const REST_HELPER: &str = r#"var __rest = (this && this.__rest) || function (s, e) {
    var t = {};
    for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p) && e.indexOf(p) < 0)
        t[p] = s[p];
    if (s != null && typeof Object.getOwnPropertySymbols === "function")
        for (var i = 0, p = Object.getOwnPropertySymbols(s); i < p.length; i++) {
            if (e.indexOf(p[i]) < 0 && Object.prototype.propertyIsEnumerable.call(s, p[i]))
                t[p[i]] = s[p[i]];
        }
    return t;
};"#;

/// Helper code for __decorate (decorators)
pub const DECORATE_HELPER: &str = r#"var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};"#;

/// Helper code for __param (parameter decorators)
pub const PARAM_HELPER: &str = r#"var __param = (this && this.__param) || function (paramIndex, decorator) {
    return function (target, key) { decorator(target, key, paramIndex); }
};"#;

/// Helper code for __metadata (reflect metadata)
pub const METADATA_HELPER: &str = r#"var __metadata = (this && this.__metadata) || function (k, v) {
    if (typeof Reflect === "object" && typeof Reflect.metadata === "function") return Reflect.metadata(k, v);
};"#;

/// Helper code for __awaiter (async/await)
pub const AWAITER_HELPER: &str = r#"var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};"#;

/// Helper code for __generator (generators)
pub const GENERATOR_HELPER: &str = r#"var __generator = (this && this.__generator) || function (thisArg, body) {
    var _ = { label: 0, sent: function() { if (t[0] & 1) throw t[1]; return t[1]; }, trys: [], ops: [] }, f, y, t, g = Object.create((typeof Iterator === "function" ? Iterator : Object).prototype);
    return g.next = verb(0), g["throw"] = verb(1), g["return"] = verb(2), typeof Symbol === "function" && (g[Symbol.iterator] = function() { return this; }), g;
    function verb(n) { return function (v) { return step([n, v]); }; }
    function step(op) {
        if (f) throw new TypeError("Generator is already executing.");
        while (g && (g = 0, op[0] && (_ = 0)), _) try {
            if (f = 1, y && (t = op[0] & 2 ? y["return"] : op[0] ? y["throw"] || ((t = y["return"]) && t.call(y), 0) : y.next) && !(t = t.call(y, op[1])).done) return t;
            if (y = 0, t) op = [op[0] & 2, t.value];
            switch (op[0]) {
                case 0: case 1: t = op; break;
                case 4: _.label++; return { value: op[1], done: false };
                case 5: _.label++; y = op[1]; op = [0]; continue;
                case 7: op = _.ops.pop(); _.trys.pop(); continue;
                default:
                    if (!(t = _.trys, t = t.length > 0 && t[t.length - 1]) && (op[0] === 6 || op[0] === 2)) { _ = 0; continue; }
                    if (op[0] === 3 && (!t || (op[1] > t[0] && op[1] < t[3]))) { _.label = op[1]; break; }
                    if (op[0] === 6 && _.label < t[1]) { _.label = t[1]; t = op; break; }
                    if (t && _.label < t[2]) { _.label = t[2]; _.ops.push(op); break; }
                    if (t[2]) _.ops.pop();
                    _.trys.pop(); continue;
            }
            op = body.call(thisArg, _);
        } catch (e) { op = [6, e]; y = 0; } finally { f = t = 0; }
        if (op[0] & 5) throw op[1]; return { value: op[0] ? op[1] : void 0, done: true };
    }
};"#;

/// Helper code for __values (for..of)
pub const VALUES_HELPER: &str = r#"var __values = (this && this.__values) || function(o) {
    var s = typeof Symbol === "function" && Symbol.iterator, m = s && o[s], i = 0;
    if (m) return m.call(o);
    if (o && typeof o.length === "number") return {
        next: function () {
            if (o && i >= o.length) o = void 0;
            return { value: o && o[i++], done: !o };
        }
    };
    throw new TypeError(s ? "Object is not iterable." : "Symbol.iterator is not defined.");
};"#;

/// Helper code for __await (async generator support)
pub const AWAIT_HELPER: &str = r#"var __await = (this && this.__await) || function (v) { return this instanceof __await ? (this.v = v, this) : new __await(v); }"#;

/// Helper code for __asyncGenerator (async generator functions)
pub const ASYNC_GENERATOR_HELPER: &str = r#"var __asyncGenerator = (this && this.__asyncGenerator) || function (thisArg, _arguments, generator) {
    if (!Symbol.asyncIterator) throw new TypeError("Symbol.asyncIterator is not defined.");
    var g = generator.apply(thisArg, _arguments || []), i, q = [];
    return i = Object.create((typeof AsyncIterator === "function" ? AsyncIterator : Object).prototype), verb("next"), verb("throw"), verb("return", awaitReturn), i[Symbol.asyncIterator] = function () { return this; }, i;
    function awaitReturn(f) { return function (v) { return Promise.resolve(v).then(f, reject); }; }
    function verb(n, f) { if (g[n]) { i[n] = function (v) { return new Promise(function (a, b) { q.push([n, v, a, b]) > 1 || resume(n, v); }); }; if (f) i[n] = f(i[n]); } }
    function resume(n, v) { try { step(g[n](v)); } catch (e) { settle(q[0][3], e); } }
    function step(r) { r.value instanceof __await ? Promise.resolve(r.value.v).then(fulfill, reject) : settle(q[0][2], r); }
    function fulfill(value) { resume("next", value); }
    function reject(value) { resume("throw", value); }
    function settle(f, v) { if (f(v), q.shift(), q.length) resume(q[0][0], q[0][1]); }
};"#;

/// Helper code for __asyncDelegator (yield* in async generators)
pub const ASYNC_DELEGATOR_HELPER: &str = r#"var __asyncDelegator = (this && this.__asyncDelegator) || function (o) {
    var i, p;
    return i = {}, verb("next"), verb("throw", function (e) { throw e; }), verb("return"), i[Symbol.iterator] = function () { return this; }, i;
    function verb(n, f) { i[n] = o[n] ? function (v) { return (p = !p) ? { value: __await(o[n](v)), done: false } : f ? f(v) : v; } : f; }
};"#;

/// Helper code for __asyncValues (for-await-of)
pub const ASYNC_VALUES_HELPER: &str = r#"var __asyncValues = (this && this.__asyncValues) || function (o) {
    if (!Symbol.asyncIterator) throw new TypeError("Symbol.asyncIterator is not defined.");
    var m = o[Symbol.asyncIterator], i;
    return m ? m.call(o) : (o = typeof __values === "function" ? __values(o) : o[Symbol.iterator](), i = {}, verb("next"), verb("throw"), verb("return"), i[Symbol.asyncIterator] = function () { return this; }, i);
    function verb(n) { i[n] = o[n] && function (v) { return new Promise(function (resolve, reject) { v = o[n](v), settle(resolve, reject, v.done, v.value); }); }; }
    function settle(resolve, reject, d, v) { Promise.resolve(v).then(function(v) { resolve({ value: v, done: d }); }, reject); }
};"#;

/// Helper code for __read (array destructuring)
pub const READ_HELPER: &str = r#"var __read = (this && this.__read) || function (o, n) {
    var m = typeof Symbol === "function" && o[Symbol.iterator];
    if (!m) return o;
    var i = m.call(o), r, ar = [], e;
    try {
        while ((n === void 0 || n-- > 0) && !(r = i.next()).done) ar.push(r.value);
    }
    catch (error) { e = { error: error }; }
    finally {
        try {
            if (r && !r.done && (m = i["return"])) m.call(i);
        }
        finally { if (e) throw e.error; }
    }
    return ar;
};"#;

/// Helper code for __spreadArray (spread in arrays)
pub const SPREAD_ARRAY_HELPER: &str = r#"var __spreadArray = (this && this.__spreadArray) || function (to, from, pack) {
    if (pack || arguments.length === 2) for (var i = 0, l = from.length, ar; i < l; i++) {
        if (ar || !(i in from)) {
            if (!ar) ar = Array.prototype.slice.call(from, 0, i);
            ar[i] = from[i];
        }
    }
    return to.concat(ar || Array.prototype.slice.call(from));
};"#;

/// Helper code for __importDefault (default imports)
pub const IMPORT_DEFAULT_HELPER: &str = r#"var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};"#;

/// Helper code for __importStar (namespace imports)
pub const IMPORT_STAR_HELPER: &str = r#"var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();"#;

/// Helper code for __exportStar (export * from)
pub const EXPORT_STAR_HELPER: &str = r#"var __exportStar = (this && this.__exportStar) || function(m, exports) {
    for (var p in m) if (p !== "default" && !Object.prototype.hasOwnProperty.call(exports, p)) __createBinding(exports, m, p);
};"#;

/// Helper code for __makeTemplateObject (tagged templates)
pub const MAKE_TEMPLATE_OBJECT_HELPER: &str = r#"var __makeTemplateObject = (this && this.__makeTemplateObject) || function (cooked, raw) {
    if (Object.defineProperty) { Object.defineProperty(cooked, "raw", { value: raw }); } else { cooked.raw = raw; }
    return cooked;
};"#;

/// Helper code for __classPrivateFieldGet (private field access)
pub const CLASS_PRIVATE_FIELD_GET_HELPER: &str = r#"var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};"#;

/// Helper code for __classPrivateFieldSet (private field assignment)
pub const CLASS_PRIVATE_FIELD_SET_HELPER: &str = r#"var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};"#;

/// Helper code for __classPrivateFieldIn (private field #field in obj check)
pub const CLASS_PRIVATE_FIELD_IN_HELPER: &str = r#"var __classPrivateFieldIn = (this && this.__classPrivateFieldIn) || function(state, receiver) {
    if (receiver === null || (typeof receiver !== "object" && typeof receiver !== "function")) throw new TypeError("Cannot use 'in' operator on non-object");
    return typeof state === "function" ? receiver === state : state.has(receiver);
};"#;

/// Helper code for __createBinding (export bindings)
pub const CREATE_BINDING_HELPER: &str = r#"var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));"#;

/// Helper code for __setModuleDefault
pub const SET_MODULE_DEFAULT_HELPER: &str = r#"var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});"#;

/// Helper code for __addDisposableResource (disposable stack helper)
pub const ADD_DISPOSABLE_RESOURCE_HELPER: &str = r#"var __addDisposableResource = (this && this.__addDisposableResource) || function (env, value, async) {
    if (value !== null && value !== void 0) {
        if (typeof value !== "object" && typeof value !== "function") throw new TypeError("Object expected.");
        var dispose, inner;
        if (async) {
            if (!Symbol.asyncDispose) throw new TypeError("Symbol.asyncDispose is not defined.");
            dispose = value[Symbol.asyncDispose];
        }
        if (dispose === void 0) {
            if (!Symbol.dispose) throw new TypeError("Symbol.dispose is not defined.");
            dispose = value[Symbol.dispose];
            if (async) inner = dispose;
        }
        if (typeof dispose !== "function") throw new TypeError("Object not disposable.");
        if (inner) dispose = function() { try { inner.call(this); } catch (e) { return Promise.reject(e); } };
        env.stack.push({ value: value, dispose: dispose, async: async });
    }
    else if (async) {
        env.stack.push({ async: true });
    }
    return value;
};"#;

/// Helper code for __disposeResources (disposable resource stack finalizer)
pub const DISPOSE_RESOURCES_HELPER: &str = r#"var __disposeResources = (this && this.__disposeResources) || (function (SuppressedError) {
    return function (env) {
        function fail(e) {
            env.error = env.hasError ? new SuppressedError(e, env.error, "An error was suppressed during disposal.") : e;
            env.hasError = true;
        }
        var r, s = 0;
        function next() {
            while (r = env.stack.pop()) {
                try {
                    if (!r.async && s === 1) return s = 0, env.stack.push(r), Promise.resolve().then(next);
                    if (r.dispose) {
                        var result = r.dispose.call(r.value);
                        if (r.async) return s |= 2, Promise.resolve(result).then(next, function(e) { fail(e); return next(); });
                    }
                    else s |= 1;
                }
                catch (e) {
                    fail(e);
                }
            }
            if (s === 1) return env.hasError ? Promise.reject(env.error) : Promise.resolve();
            if (env.hasError) throw env.error;
        }
        return next();
    };
})(typeof SuppressedError === "function" ? SuppressedError : function (error, suppressed, message) {
    var e = new Error(message);
    return e.name = "SuppressedError", e.error = error, e.suppressed = suppressed, e;
});"#;

/// Helper code for __esDecorate (TC39 decorators)
pub const ES_DECORATE_HELPER: &str = r#"var __esDecorate = (this && this.__esDecorate) || function (ctor, descriptorIn, decorators, contextIn, initializers, extraInitializers) {
    function accept(f) { if (f !== void 0 && typeof f !== "function") throw new TypeError("Function expected"); return f; }
    var kind = contextIn.kind, key = kind === "getter" ? "get" : kind === "setter" ? "set" : "value";
    var target = !descriptorIn && ctor ? contextIn["static"] ? ctor : ctor.prototype : null;
    var descriptor = descriptorIn || (target ? Object.getOwnPropertyDescriptor(target, contextIn.name) : {});
    var _, done = false;
    for (var i = decorators.length - 1; i >= 0; i--) {
        var context = {};
        for (var p in contextIn) context[p] = p === "access" ? {} : contextIn[p];
        for (var p in contextIn.access) context.access[p] = contextIn.access[p];
        context.addInitializer = function (f) { if (done) throw new TypeError("Cannot add initializers after decoration has completed"); extraInitializers.push(accept(f || null)); };
        var result = (0, decorators[i])(kind === "accessor" ? { get: descriptor.get, set: descriptor.set } : descriptor[key], context);
        if (kind === "accessor") {
            if (result === void 0) continue;
            if (result === null || typeof result !== "object") throw new TypeError("Object expected");
            if (_ = accept(result.get)) descriptor.get = _;
            if (_ = accept(result.set)) descriptor.set = _;
            if (_ = accept(result.init)) initializers.unshift(_);
        }
        else if (_ = accept(result)) {
            if (kind === "field") initializers.unshift(_);
            else descriptor[key] = _;
        }
    }
    if (target) Object.defineProperty(target, contextIn.name, descriptor);
    done = true;
};"#;

/// Helper code for __runInitializers (TC39 decorators)
pub const RUN_INITIALIZERS_HELPER: &str = r#"var __runInitializers = (this && this.__runInitializers) || function (thisArg, initializers, value) {
    var useValue = arguments.length > 2;
    for (var i = 0; i < initializers.length; i++) {
        value = useValue ? initializers[i].call(thisArg, value) : initializers[i].call(thisArg);
    }
    return useValue ? value : void 0;
};"#;

/// Helper code for __propKey (TC39 decorators - computed property key)
pub const PROP_KEY_HELPER: &str = r#"var __propKey = (this && this.__propKey) || function (x) {
    return typeof x === "symbol" ? x : "".concat(x);
};"#;

/// Helper code for __setFunctionName (TC39 decorators)
pub const SET_FUNCTION_NAME_HELPER: &str = r#"var __setFunctionName = (this && this.__setFunctionName) || function (f, name, prefix) {
    if (typeof name === "symbol") name = name.description ? "[".concat(name.description, "]") : "";
    return Object.defineProperty(f, "name", { configurable: true, value: prefix ? "".concat(prefix, " ", name) : name });
};"#;

/// Helper code for __rewriteRelativeImportExtension (dynamic import/require specifier rewriting)
pub const REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER: &str = r#"var __rewriteRelativeImportExtension = (this && this.__rewriteRelativeImportExtension) || function (path, preserveJsx) {
    if (typeof path === "string" && /^\.\.?\//.test(path)) {
        return path.replace(/\.(tsx)$|((?:\.d)?)((?:\.[^./]+?)?)\.([cm]?)ts$/i, function (m, tsx, d, ext, cm) {
            return tsx ? preserveJsx ? ".jsx" : ".js" : d && (!ext || !cm) ? m : (d + ext + "." + cm.toLowerCase() + "js");
        });
    }
    return path;
};"#;

/// Tracks which helper functions are needed in the output.
#[derive(Default, Clone)]
pub struct HelpersNeeded {
    pub extends: bool,
    pub assign: bool,
    pub rest: bool,
    pub decorate: bool,
    pub param: bool,
    pub metadata: bool,
    pub awaiter: bool,
    pub generator: bool,
    pub values: bool,
    pub read: bool,
    pub spread_array: bool,
    pub async_values: bool,
    pub async_generator: bool,
    pub async_delegator: bool,
    pub await_helper: bool,
    pub export_star: bool,
    pub import_default: bool,
    pub import_star: bool,
    pub make_template_object: bool,
    pub class_private_field_get: bool,
    pub class_private_field_set: bool,
    /// Whether Set was registered before Get (controls helper emit order).
    /// tsc emits helpers in first-use order; when the first private field
    /// operation is a plain assignment, Set appears before Get.
    pub class_private_field_set_before_get: bool,
    pub class_private_field_in: bool,
    pub create_binding: bool,
    pub add_disposable_resource: bool,
    pub dispose_resources: bool,
    pub es_decorate: bool,
    pub run_initializers: bool,
    pub prop_key: bool,
    pub set_function_name: bool,
    pub rewrite_relative_import_extension: bool,
}

impl HelpersNeeded {
    /// Returns true if any helper is needed.
    pub const fn any_needed(&self) -> bool {
        self.extends
            || self.assign
            || self.rest
            || self.decorate
            || self.param
            || self.metadata
            || self.awaiter
            || self.generator
            || self.values
            || self.read
            || self.spread_array
            || self.async_values
            || self.async_generator
            || self.async_delegator
            || self.await_helper
            || self.export_star
            || self.import_default
            || self.import_star
            || self.make_template_object
            || self.class_private_field_get
            || self.class_private_field_set
            || self.class_private_field_in
            || self.create_binding
            || self.add_disposable_resource
            || self.dispose_resources
            || self.es_decorate
            || self.run_initializers
            || self.prop_key
            || self.set_function_name
            || self.rewrite_relative_import_extension
    }

    /// Returns the list of `__helperName` strings for all needed helpers.
    /// Used for generating `import { ... } from "tslib"` statements.
    pub fn needed_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.extends {
            names.push("__extends");
        }
        if self.make_template_object {
            names.push("__makeTemplateObject");
        }
        if self.assign {
            names.push("__assign");
        }
        if self.create_binding {
            names.push("__createBinding");
        }
        if self.decorate {
            names.push("__decorate");
        }
        if self.es_decorate {
            names.push("__esDecorate");
        }
        if self.run_initializers {
            names.push("__runInitializers");
        }
        if self.import_star {
            names.push("__importStar");
        }
        if self.export_star {
            names.push("__exportStar");
        }
        if self.metadata {
            names.push("__metadata");
        }
        if self.param {
            names.push("__param");
        }
        if self.awaiter {
            names.push("__awaiter");
        }
        if self.generator {
            names.push("__generator");
        }
        if self.await_helper {
            names.push("__await");
        }
        if self.async_generator {
            names.push("__asyncGenerator");
        }
        if self.async_delegator {
            names.push("__asyncDelegator");
        }
        if self.rest {
            names.push("__rest");
        }
        if self.values {
            names.push("__values");
        }
        if self.read {
            names.push("__read");
        }
        if self.spread_array {
            names.push("__spreadArray");
        }
        if self.async_values {
            names.push("__asyncValues");
        }
        if self.import_default {
            names.push("__importDefault");
        }
        if self.class_private_field_get {
            names.push("__classPrivateFieldGet");
        }
        if self.class_private_field_set {
            names.push("__classPrivateFieldSet");
        }
        if self.class_private_field_in {
            names.push("__classPrivateFieldIn");
        }
        if self.add_disposable_resource {
            names.push("__addDisposableResource");
        }
        if self.dispose_resources {
            names.push("__disposeResources");
        }
        if self.prop_key {
            names.push("__propKey");
        }
        if self.set_function_name {
            names.push("__setFunctionName");
        }
        if self.rewrite_relative_import_extension {
            names.push("__rewriteRelativeImportExtension");
        }
        names
    }
}

/// Generate helper code for the needed helpers.
///
/// Ordering follows TypeScript's `compareEmitHelpers` priority system.
/// Helpers with a defined priority come first (lower number = earlier).
/// Helpers with no priority (undefined in tsc) come last.
///
/// Priority mapping (from TypeScript's factory/emitHelpers.ts):
///   0: extends, makeTemplateObject
///   1: assign, createBinding
///   2: decorate, esDecorate, runInitializers, importStar, exportStar
///   3: metadata
///   4: param
///   5: awaiter
///   6: generator
///  no priority (last): rest, read, spreadArray, values, asyncValues,
///                      importDefault, classPrivateField*, disposable helpers
pub fn emit_helpers(helpers: &HelpersNeeded) -> String {
    let mut output = String::new();

    // Priority 0: extends, makeTemplateObject
    if helpers.extends {
        output.push_str(EXTENDS_HELPER);
        output.push('\n');
    }
    if helpers.make_template_object {
        output.push_str(MAKE_TEMPLATE_OBJECT_HELPER);
        output.push('\n');
    }
    // Priority 1: assign, createBinding
    if helpers.assign {
        output.push_str(ASSIGN_HELPER);
        output.push('\n');
    }
    if helpers.create_binding {
        output.push_str(CREATE_BINDING_HELPER);
        output.push('\n');
    }
    // Priority 2: decorate, esDecorate, runInitializers, importStar (with setModuleDefault), exportStar
    if helpers.decorate {
        output.push_str(DECORATE_HELPER);
        output.push('\n');
    }
    if helpers.run_initializers {
        output.push_str(RUN_INITIALIZERS_HELPER);
        output.push('\n');
    }
    if helpers.es_decorate {
        output.push_str(ES_DECORATE_HELPER);
        output.push('\n');
    }
    if helpers.set_function_name {
        output.push_str(SET_FUNCTION_NAME_HELPER);
        output.push('\n');
    }
    if helpers.prop_key {
        output.push_str(PROP_KEY_HELPER);
        output.push('\n');
    }
    if helpers.import_star {
        output.push_str(SET_MODULE_DEFAULT_HELPER);
        output.push('\n');
        output.push_str(IMPORT_STAR_HELPER);
        output.push('\n');
    }
    if helpers.rewrite_relative_import_extension {
        output.push_str(REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER);
        output.push('\n');
    }
    if helpers.export_star {
        output.push_str(EXPORT_STAR_HELPER);
        output.push('\n');
    }
    // Priority 3: metadata
    if helpers.metadata {
        output.push_str(METADATA_HELPER);
        output.push('\n');
    }
    // Priority 4: param
    if helpers.param {
        output.push_str(PARAM_HELPER);
        output.push('\n');
    }
    // Priority 5: awaiter
    if helpers.awaiter {
        output.push_str(AWAITER_HELPER);
        output.push('\n');
    }
    // Priority 6: generator
    if helpers.generator {
        output.push_str(GENERATOR_HELPER);
        output.push('\n');
    }
    // Async generator helpers (after awaiter/generator)
    if helpers.await_helper {
        output.push_str(AWAIT_HELPER);
        output.push('\n');
    }
    if helpers.async_generator {
        output.push_str(ASYNC_GENERATOR_HELPER);
        output.push('\n');
    }
    if helpers.async_delegator {
        output.push_str(ASYNC_DELEGATOR_HELPER);
        output.push('\n');
    }
    // No priority (come last in tsc order): rest, values, read, spreadArray,
    // asyncValues, importDefault, classPrivateField*, disposable helpers
    if helpers.rest {
        output.push_str(REST_HELPER);
        output.push('\n');
    }
    if helpers.values {
        output.push_str(VALUES_HELPER);
        output.push('\n');
    }
    if helpers.read {
        output.push_str(READ_HELPER);
        output.push('\n');
    }
    if helpers.spread_array {
        output.push_str(SPREAD_ARRAY_HELPER);
        output.push('\n');
    }
    if helpers.import_default {
        output.push_str(IMPORT_DEFAULT_HELPER);
        output.push('\n');
    }
    // Emit Get/Set helpers in first-use order (tsc tracks insertion order)
    if helpers.class_private_field_set_before_get {
        if helpers.class_private_field_set {
            output.push_str(CLASS_PRIVATE_FIELD_SET_HELPER);
            output.push('\n');
        }
        if helpers.class_private_field_get {
            output.push_str(CLASS_PRIVATE_FIELD_GET_HELPER);
            output.push('\n');
        }
    } else {
        if helpers.class_private_field_get {
            output.push_str(CLASS_PRIVATE_FIELD_GET_HELPER);
            output.push('\n');
        }
        if helpers.class_private_field_set {
            output.push_str(CLASS_PRIVATE_FIELD_SET_HELPER);
            output.push('\n');
        }
    }
    if helpers.class_private_field_in {
        output.push_str(CLASS_PRIVATE_FIELD_IN_HELPER);
        output.push('\n');
    }
    if helpers.add_disposable_resource {
        output.push_str(ADD_DISPOSABLE_RESOURCE_HELPER);
        output.push('\n');
    }
    if helpers.dispose_resources {
        output.push_str(DISPOSE_RESOURCES_HELPER);
        output.push('\n');
    }
    if helpers.async_values {
        output.push_str(ASYNC_VALUES_HELPER);
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Setter callback used by the per-flag `any_needed` test.
    type FlagSetter = fn(&mut HelpersNeeded);

    // -----------------------------------------------------------------
    // HelpersNeeded::any_needed
    // -----------------------------------------------------------------

    #[test]
    fn default_helpers_needed_is_false() {
        let helpers = HelpersNeeded::default();
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
        assert!(emit_helpers(&helpers).is_empty());
    }

    #[test]
    fn any_needed_flips_for_each_individual_flag() {
        // Each individual flag, when set in isolation, must flip `any_needed`
        // to true. This guards against `any_needed` forgetting to OR a new
        // field after someone adds a new helper to `HelpersNeeded`.
        let setters: &[(&str, FlagSetter)] = &[
            ("extends", |h| h.extends = true),
            ("assign", |h| h.assign = true),
            ("rest", |h| h.rest = true),
            ("decorate", |h| h.decorate = true),
            ("param", |h| h.param = true),
            ("metadata", |h| h.metadata = true),
            ("awaiter", |h| h.awaiter = true),
            ("generator", |h| h.generator = true),
            ("values", |h| h.values = true),
            ("read", |h| h.read = true),
            ("spread_array", |h| h.spread_array = true),
            ("async_values", |h| h.async_values = true),
            ("async_generator", |h| h.async_generator = true),
            ("async_delegator", |h| h.async_delegator = true),
            ("await_helper", |h| h.await_helper = true),
            ("export_star", |h| h.export_star = true),
            ("import_default", |h| h.import_default = true),
            ("import_star", |h| h.import_star = true),
            ("make_template_object", |h| h.make_template_object = true),
            ("class_private_field_get", |h| {
                h.class_private_field_get = true
            }),
            ("class_private_field_set", |h| {
                h.class_private_field_set = true
            }),
            ("class_private_field_in", |h| {
                h.class_private_field_in = true
            }),
            ("create_binding", |h| h.create_binding = true),
            ("add_disposable_resource", |h| {
                h.add_disposable_resource = true
            }),
            ("dispose_resources", |h| h.dispose_resources = true),
            ("es_decorate", |h| h.es_decorate = true),
            ("run_initializers", |h| h.run_initializers = true),
            ("prop_key", |h| h.prop_key = true),
            ("set_function_name", |h| h.set_function_name = true),
            ("rewrite_relative_import_extension", |h| {
                h.rewrite_relative_import_extension = true;
            }),
        ];

        for (name, setter) in setters {
            let mut helpers = HelpersNeeded::default();
            setter(&mut helpers);
            assert!(
                helpers.any_needed(),
                "any_needed() should be true when only `{name}` is set",
            );
        }
    }

    #[test]
    fn class_private_field_set_before_get_alone_does_not_trigger_any_needed() {
        // The ordering flag is bookkeeping for emit ordering only — by itself
        // it should NOT make any_needed() return true, otherwise the emitter
        // would erroneously install a tslib import for a no-op state.
        let helpers = HelpersNeeded {
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };
        assert!(!helpers.any_needed());
        assert!(helpers.needed_names().is_empty());
    }

    // -----------------------------------------------------------------
    // HelpersNeeded::needed_names
    // -----------------------------------------------------------------

    #[test]
    fn needed_names_returns_canonical_helper_strings() {
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            awaiter: true,
            ..HelpersNeeded::default()
        };

        let names = helpers.needed_names();
        assert_eq!(names, vec!["__extends", "__assign", "__awaiter"]);
    }

    #[test]
    fn needed_names_priority_order_for_full_set() {
        // When every flag is set, `needed_names` must produce the names in
        // the documented `compareEmitHelpers` priority order.
        let helpers = HelpersNeeded {
            extends: true,
            make_template_object: true,
            assign: true,
            create_binding: true,
            decorate: true,
            es_decorate: true,
            run_initializers: true,
            import_star: true,
            export_star: true,
            metadata: true,
            param: true,
            awaiter: true,
            generator: true,
            await_helper: true,
            async_generator: true,
            async_delegator: true,
            rest: true,
            values: true,
            read: true,
            spread_array: true,
            async_values: true,
            import_default: true,
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: false,
            class_private_field_in: true,
            add_disposable_resource: true,
            dispose_resources: true,
            prop_key: true,
            set_function_name: true,
            rewrite_relative_import_extension: true,
        };

        let names = helpers.needed_names();
        // Lock the full canonical order. This regression-catches a missing
        // entry, an out-of-order entry, or a duplicate.
        assert_eq!(
            names,
            vec![
                "__extends",
                "__makeTemplateObject",
                "__assign",
                "__createBinding",
                "__decorate",
                "__esDecorate",
                "__runInitializers",
                "__importStar",
                "__exportStar",
                "__metadata",
                "__param",
                "__awaiter",
                "__generator",
                "__await",
                "__asyncGenerator",
                "__asyncDelegator",
                "__rest",
                "__values",
                "__read",
                "__spreadArray",
                "__asyncValues",
                "__importDefault",
                "__classPrivateFieldGet",
                "__classPrivateFieldSet",
                "__classPrivateFieldIn",
                "__addDisposableResource",
                "__disposeResources",
                "__propKey",
                "__setFunctionName",
                "__rewriteRelativeImportExtension",
            ],
        );
    }

    #[test]
    fn needed_names_skips_unset_flags() {
        let helpers = HelpersNeeded {
            assign: true,
            spread_array: true,
            ..HelpersNeeded::default()
        };

        let names = helpers.needed_names();
        // Only the two set helpers, in priority order (`__assign` is priority
        // 1, `__spreadArray` is unprioritized so it comes later).
        assert_eq!(names, vec!["__assign", "__spreadArray"]);
    }

    // -----------------------------------------------------------------
    // emit_helpers priority ordering
    // -----------------------------------------------------------------

    /// Find the byte offset of a helper's `var __name` declaration in the
    /// emitted source. Asserts the helper is present.
    fn find_helper(output: &str, name: &str) -> usize {
        let needle = format!("var {name}");
        output
            .find(&needle)
            .unwrap_or_else(|| panic!("expected `{needle}` in emit_helpers output:\n{output}"))
    }

    #[test]
    fn emit_helpers_priority_order_extends_assign_decorate_metadata_param_awaiter_generator() {
        // tsc priority order (from helpers.rs doc-comment):
        //   0: extends, makeTemplateObject
        //   1: assign, createBinding
        //   2: decorate, esDecorate, runInitializers, importStar, exportStar
        //   3: metadata
        //   4: param
        //   5: awaiter
        //   6: generator
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            decorate: true,
            metadata: true,
            param: true,
            awaiter: true,
            generator: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);

        let i_extends = find_helper(&output, "__extends");
        let i_assign = find_helper(&output, "__assign");
        let i_decorate = find_helper(&output, "__decorate");
        let i_metadata = find_helper(&output, "__metadata");
        let i_param = find_helper(&output, "__param");
        let i_awaiter = find_helper(&output, "__awaiter");
        let i_generator = find_helper(&output, "__generator");

        assert!(
            i_extends < i_assign
                && i_assign < i_decorate
                && i_decorate < i_metadata
                && i_metadata < i_param
                && i_param < i_awaiter
                && i_awaiter < i_generator,
            "priority order broken: extends={i_extends} assign={i_assign} \
             decorate={i_decorate} metadata={i_metadata} param={i_param} \
             awaiter={i_awaiter} generator={i_generator}\noutput:\n{output}",
        );
    }

    #[test]
    fn emit_helpers_priority_zero_extends_before_make_template_object() {
        // Both share priority 0; doc-comment + emit_helpers source state that
        // extends comes first within priority 0 (matches tsc factory order).
        let helpers = HelpersNeeded {
            extends: true,
            make_template_object: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_extends = find_helper(&output, "__extends");
        let i_make = find_helper(&output, "__makeTemplateObject");
        assert!(i_extends < i_make);
    }

    #[test]
    fn emit_helpers_priority_one_assign_before_create_binding() {
        // Both share priority 1; assign first.
        let helpers = HelpersNeeded {
            assign: true,
            create_binding: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_assign = find_helper(&output, "__assign");
        let i_create = find_helper(&output, "__createBinding");
        assert!(i_assign < i_create);
    }

    #[test]
    fn emit_helpers_priority_two_internal_order_decorate_run_init_es_decorate() {
        // emit_helpers source orders priority-2 helpers as:
        //   decorate, runInitializers, esDecorate, setFunctionName,
        //   propKey, importStar (with setModuleDefault),
        //   rewriteRelativeImportExtension, exportStar
        let helpers = HelpersNeeded {
            decorate: true,
            run_initializers: true,
            es_decorate: true,
            set_function_name: true,
            prop_key: true,
            import_star: true,
            rewrite_relative_import_extension: true,
            export_star: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_decorate = find_helper(&output, "__decorate");
        let i_run = find_helper(&output, "__runInitializers");
        let i_es_decorate = find_helper(&output, "__esDecorate");
        let i_set_name = find_helper(&output, "__setFunctionName");
        let i_prop_key = find_helper(&output, "__propKey");
        let i_import_star = find_helper(&output, "__importStar");
        let i_rewrite = find_helper(&output, "__rewriteRelativeImportExtension");
        let i_export_star = find_helper(&output, "__exportStar");

        assert!(i_decorate < i_run);
        assert!(i_run < i_es_decorate);
        assert!(i_es_decorate < i_set_name);
        assert!(i_set_name < i_prop_key);
        assert!(i_prop_key < i_import_star);
        assert!(i_import_star < i_rewrite);
        assert!(i_rewrite < i_export_star);
    }

    #[test]
    fn emit_helpers_no_priority_block_emits_last() {
        // The unprioritized block should emit AFTER any prioritized helper.
        let helpers = HelpersNeeded {
            // Priority 6 (last prioritized).
            generator: true,
            // Unprioritized block:
            rest: true,
            values: true,
            read: true,
            spread_array: true,
            import_default: true,
            async_values: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_generator = find_helper(&output, "__generator");
        let i_rest = find_helper(&output, "__rest");
        let i_values = find_helper(&output, "__values");
        let i_read = find_helper(&output, "__read");
        let i_spread = find_helper(&output, "__spreadArray");
        let i_import_default = find_helper(&output, "__importDefault");
        let i_async_values = find_helper(&output, "__asyncValues");

        assert!(i_generator < i_rest, "generator must precede rest");
        assert!(i_rest < i_values);
        assert!(i_values < i_read);
        assert!(i_read < i_spread);
        assert!(i_spread < i_import_default);
        // async_values is emitted after disposable helpers, which are also
        // unprioritized; in this configuration without disposable helpers,
        // async_values still comes after import_default.
        assert!(i_import_default < i_async_values);
    }

    #[test]
    fn emit_helpers_class_private_field_default_get_before_set() {
        // Default ordering: Get before Set.
        let helpers = HelpersNeeded {
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: false,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set = find_helper(&output, "__classPrivateFieldSet");
        assert!(
            i_get < i_set,
            "default order should put Get before Set, got Get={i_get} Set={i_set}",
        );
    }

    #[test]
    fn emit_helpers_class_private_field_set_before_get_flips_order() {
        // When set_before_get is true (set was registered first), Set emits
        // before Get.
        let helpers = HelpersNeeded {
            class_private_field_get: true,
            class_private_field_set: true,
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_get = find_helper(&output, "__classPrivateFieldGet");
        let i_set = find_helper(&output, "__classPrivateFieldSet");
        assert!(
            i_set < i_get,
            "set_before_get=true should put Set before Get, got Get={i_get} Set={i_set}",
        );
    }

    #[test]
    fn emit_helpers_class_private_field_set_before_get_only_set_emits_only_set() {
        // Even with set_before_get=true, if only Set is needed, only Set is emitted.
        let helpers = HelpersNeeded {
            class_private_field_set: true,
            class_private_field_set_before_get: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        assert!(output.contains("var __classPrivateFieldSet"));
        assert!(!output.contains("var __classPrivateFieldGet"));
    }

    #[test]
    fn emit_helpers_import_star_emits_set_module_default_before_import_star() {
        // import_star=true should emit BOTH __setModuleDefault and __importStar,
        // with __setModuleDefault first (since __importStar references it).
        let helpers = HelpersNeeded {
            import_star: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_set_default = find_helper(&output, "__setModuleDefault");
        let i_import_star = find_helper(&output, "__importStar");
        assert!(
            i_set_default < i_import_star,
            "__setModuleDefault must precede __importStar (referenced by it)",
        );
    }

    #[test]
    fn emit_helpers_each_helper_terminated_by_newline() {
        // Each emitted helper string is followed by a newline so consecutive
        // helpers don't run together on the same line.
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        // The output should end with a newline.
        assert!(output.ends_with('\n'));
        // It should contain both helper bodies.
        assert!(output.contains("var __extends"));
        assert!(output.contains("var __assign"));
        // And each `var __` line should be at the start of a line (preceded
        // by a newline) except the very first one.
        let first = output.find("var __extends").expect("__extends present");
        let assign_pos = output.find("var __assign").expect("__assign present");
        // The byte before `var __assign` must be a newline.
        assert_eq!(&output[assign_pos - 1..assign_pos], "\n");
        // First helper starts at offset 0.
        assert_eq!(first, 0);
    }

    #[test]
    fn emit_helpers_disposable_resource_pair_ordered_add_before_dispose() {
        // add_disposable_resource emits before dispose_resources.
        let helpers = HelpersNeeded {
            add_disposable_resource: true,
            dispose_resources: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        let i_add = find_helper(&output, "__addDisposableResource");
        let i_dispose = find_helper(&output, "__disposeResources");
        assert!(i_add < i_dispose);
    }

    // -----------------------------------------------------------------
    // Helper constant invariants
    // -----------------------------------------------------------------

    /// Every public helper constant should be a non-empty `var __<name>` JS
    /// declaration so that `emit_helpers` produces valid JavaScript when the
    /// constant is concatenated.
    #[test]
    fn helper_constants_are_var_declarations() {
        let cases: &[(&str, &str)] = &[
            ("EXTENDS_HELPER", EXTENDS_HELPER),
            ("ASSIGN_HELPER", ASSIGN_HELPER),
            ("REST_HELPER", REST_HELPER),
            ("DECORATE_HELPER", DECORATE_HELPER),
            ("PARAM_HELPER", PARAM_HELPER),
            ("METADATA_HELPER", METADATA_HELPER),
            ("AWAITER_HELPER", AWAITER_HELPER),
            ("GENERATOR_HELPER", GENERATOR_HELPER),
            ("VALUES_HELPER", VALUES_HELPER),
            ("AWAIT_HELPER", AWAIT_HELPER),
            ("ASYNC_GENERATOR_HELPER", ASYNC_GENERATOR_HELPER),
            ("ASYNC_DELEGATOR_HELPER", ASYNC_DELEGATOR_HELPER),
            ("ASYNC_VALUES_HELPER", ASYNC_VALUES_HELPER),
            ("READ_HELPER", READ_HELPER),
            ("SPREAD_ARRAY_HELPER", SPREAD_ARRAY_HELPER),
            ("IMPORT_DEFAULT_HELPER", IMPORT_DEFAULT_HELPER),
            ("IMPORT_STAR_HELPER", IMPORT_STAR_HELPER),
            ("EXPORT_STAR_HELPER", EXPORT_STAR_HELPER),
            ("MAKE_TEMPLATE_OBJECT_HELPER", MAKE_TEMPLATE_OBJECT_HELPER),
            (
                "CLASS_PRIVATE_FIELD_GET_HELPER",
                CLASS_PRIVATE_FIELD_GET_HELPER,
            ),
            (
                "CLASS_PRIVATE_FIELD_SET_HELPER",
                CLASS_PRIVATE_FIELD_SET_HELPER,
            ),
            (
                "CLASS_PRIVATE_FIELD_IN_HELPER",
                CLASS_PRIVATE_FIELD_IN_HELPER,
            ),
            ("CREATE_BINDING_HELPER", CREATE_BINDING_HELPER),
            ("SET_MODULE_DEFAULT_HELPER", SET_MODULE_DEFAULT_HELPER),
            (
                "ADD_DISPOSABLE_RESOURCE_HELPER",
                ADD_DISPOSABLE_RESOURCE_HELPER,
            ),
            ("DISPOSE_RESOURCES_HELPER", DISPOSE_RESOURCES_HELPER),
            ("ES_DECORATE_HELPER", ES_DECORATE_HELPER),
            ("RUN_INITIALIZERS_HELPER", RUN_INITIALIZERS_HELPER),
            ("PROP_KEY_HELPER", PROP_KEY_HELPER),
            ("SET_FUNCTION_NAME_HELPER", SET_FUNCTION_NAME_HELPER),
            (
                "REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER",
                REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER,
            ),
        ];

        for (name, body) in cases {
            assert!(!body.is_empty(), "{name} should not be empty");
            assert!(
                body.starts_with("var __"),
                "{name} should start with `var __`, got: {head:?}",
                head = &body[..body.len().min(20)],
            );
        }
    }

    #[test]
    fn helper_constants_match_needed_names_basenames() {
        // The helper constant body should declare a function whose name
        // matches `__<base>` for the `needed_names` entry that triggers it.
        // Spot-check the priority-0/1 helpers + the async block.
        let pairs: &[(&str, &str)] = &[
            ("__extends", EXTENDS_HELPER),
            ("__makeTemplateObject", MAKE_TEMPLATE_OBJECT_HELPER),
            ("__assign", ASSIGN_HELPER),
            ("__createBinding", CREATE_BINDING_HELPER),
            ("__decorate", DECORATE_HELPER),
            ("__esDecorate", ES_DECORATE_HELPER),
            ("__runInitializers", RUN_INITIALIZERS_HELPER),
            ("__metadata", METADATA_HELPER),
            ("__param", PARAM_HELPER),
            ("__awaiter", AWAITER_HELPER),
            ("__generator", GENERATOR_HELPER),
            ("__await", AWAIT_HELPER),
            ("__asyncGenerator", ASYNC_GENERATOR_HELPER),
            ("__asyncDelegator", ASYNC_DELEGATOR_HELPER),
            ("__asyncValues", ASYNC_VALUES_HELPER),
            ("__rest", REST_HELPER),
            ("__values", VALUES_HELPER),
            ("__read", READ_HELPER),
            ("__spreadArray", SPREAD_ARRAY_HELPER),
            ("__importDefault", IMPORT_DEFAULT_HELPER),
            ("__importStar", IMPORT_STAR_HELPER),
            ("__exportStar", EXPORT_STAR_HELPER),
            ("__classPrivateFieldGet", CLASS_PRIVATE_FIELD_GET_HELPER),
            ("__classPrivateFieldSet", CLASS_PRIVATE_FIELD_SET_HELPER),
            ("__classPrivateFieldIn", CLASS_PRIVATE_FIELD_IN_HELPER),
            ("__addDisposableResource", ADD_DISPOSABLE_RESOURCE_HELPER),
            ("__disposeResources", DISPOSE_RESOURCES_HELPER),
            ("__propKey", PROP_KEY_HELPER),
            ("__setFunctionName", SET_FUNCTION_NAME_HELPER),
            (
                "__rewriteRelativeImportExtension",
                REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER,
            ),
            ("__setModuleDefault", SET_MODULE_DEFAULT_HELPER),
        ];

        for (name, body) in pairs {
            let expected_prefix = format!("var {name} ");
            assert!(
                body.starts_with(&expected_prefix),
                "{name} body should start with `{expected_prefix}`, got: {head:?}",
                head = &body[..body.len().min(50)],
            );
        }
    }

    #[test]
    fn emit_helpers_round_trips_through_string_concat() {
        // emit_helpers output must contain each requested helper exactly
        // once (no accidental duplicate emission).
        let helpers = HelpersNeeded {
            extends: true,
            assign: true,
            awaiter: true,
            generator: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        for marker in [
            "var __extends",
            "var __assign",
            "var __awaiter",
            "var __generator",
        ] {
            assert_eq!(
                output.matches(marker).count(),
                1,
                "marker `{marker}` should appear exactly once in output:\n{output}",
            );
        }
    }

    #[test]
    fn emit_helpers_only_emits_requested_helpers() {
        let helpers = HelpersNeeded {
            extends: true,
            ..HelpersNeeded::default()
        };

        let output = emit_helpers(&helpers);
        assert!(output.contains("var __extends"));
        // Spot-check a few helpers that should NOT be present.
        assert!(!output.contains("var __assign"));
        assert!(!output.contains("var __awaiter"));
        assert!(!output.contains("var __decorate"));
        assert!(!output.contains("var __setModuleDefault"));
    }

    // -----------------------------------------------------------------
    // any_needed × needed_names × emit_helpers triangle
    // -----------------------------------------------------------------

    #[test]
    fn any_needed_implies_non_empty_needed_names_and_emit() {
        // For every individual flag that flips any_needed, both
        // needed_names() and emit_helpers() must produce non-empty output.
        let setters: &[fn(&mut HelpersNeeded)] = &[
            |h| h.extends = true,
            |h| h.assign = true,
            |h| h.awaiter = true,
            |h| h.generator = true,
            |h| h.import_star = true,
            |h| h.class_private_field_get = true,
            |h| h.dispose_resources = true,
            |h| h.rewrite_relative_import_extension = true,
        ];

        for setter in setters {
            let mut helpers = HelpersNeeded::default();
            setter(&mut helpers);
            assert!(helpers.any_needed());
            assert!(!helpers.needed_names().is_empty());
            assert!(!emit_helpers(&helpers).is_empty());
        }
    }

    #[test]
    fn helpers_needed_clone_round_trips() {
        // HelpersNeeded derives Clone; ensure cloning preserves all flags.
        let original = HelpersNeeded {
            extends: true,
            class_private_field_set_before_get: true,
            generator: true,
            ..HelpersNeeded::default()
        };
        let cloned = original.clone();
        assert_eq!(cloned.extends, original.extends);
        assert_eq!(
            cloned.class_private_field_set_before_get,
            original.class_private_field_set_before_get,
        );
        assert_eq!(cloned.generator, original.generator);
        assert_eq!(cloned.needed_names(), original.needed_names());
        assert_eq!(emit_helpers(&cloned), emit_helpers(&original));
    }
}
