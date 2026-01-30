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
    pub spread: bool,
    pub spread_arrays: bool,
    pub spread_array: bool,
    pub await_values: bool,
    pub async_generator: bool,
    pub async_delegator: bool,
    pub async_values: bool,
    pub export_star: bool,
    pub import_default: bool,
    pub import_star: bool,
    pub make_template_object: bool,
    pub class_private_field_get: bool,
    pub class_private_field_set: bool,
    pub class_private_field_in: bool,
    pub create_binding: bool,
    pub set_function_name: bool,
    pub prop_key: bool,
}

/// Generate helper code for the needed helpers
pub fn emit_helpers(helpers: &HelpersNeeded) -> String {
    let mut output = String::new();

    // Order matters - some helpers depend on others
    if helpers.create_binding {
        output.push_str(CREATE_BINDING_HELPER);
        output.push('\n');
    }
    if helpers.extends {
        output.push_str(EXTENDS_HELPER);
        output.push('\n');
    }
    if helpers.assign {
        output.push_str(ASSIGN_HELPER);
        output.push('\n');
    }
    if helpers.rest {
        output.push_str(REST_HELPER);
        output.push('\n');
    }
    if helpers.decorate {
        output.push_str(DECORATE_HELPER);
        output.push('\n');
    }
    if helpers.param {
        output.push_str(PARAM_HELPER);
        output.push('\n');
    }
    if helpers.metadata {
        output.push_str(METADATA_HELPER);
        output.push('\n');
    }
    if helpers.awaiter {
        output.push_str(AWAITER_HELPER);
        output.push('\n');
    }
    if helpers.generator {
        output.push_str(GENERATOR_HELPER);
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
    if helpers.import_star {
        output.push_str(SET_MODULE_DEFAULT_HELPER);
        output.push('\n');
        output.push_str(IMPORT_STAR_HELPER);
        output.push('\n');
    }
    if helpers.export_star {
        output.push_str(EXPORT_STAR_HELPER);
        output.push('\n');
    }
    if helpers.make_template_object {
        output.push_str(MAKE_TEMPLATE_OBJECT_HELPER);
        output.push('\n');
    }
    if helpers.class_private_field_get {
        output.push_str(CLASS_PRIVATE_FIELD_GET_HELPER);
        output.push('\n');
    }
    if helpers.class_private_field_set {
        output.push_str(CLASS_PRIVATE_FIELD_SET_HELPER);
        output.push('\n');
    }
    if helpers.class_private_field_in {
        output.push_str(CLASS_PRIVATE_FIELD_IN_HELPER);
        output.push('\n');
    }

    output
}

#[cfg(test)]
#[path = "tests/helpers_tests.rs"]
mod helpers_tests;
