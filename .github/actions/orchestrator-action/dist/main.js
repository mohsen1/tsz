"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
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
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.run = run;
const core = __importStar(require("@actions/core"));
const director_1 = require("./director");
const architect_1 = require("./architect");
const worker_1 = require("./worker");
const reviewer_1 = require("./reviewer");
async function run() {
    const role = core.getInput('role');
    console.log(`🤖 Booting Agent with Role: [${role.toUpperCase()}]`);
    try {
        switch (role) {
            case 'director':
                await (0, director_1.run)();
                break;
            case 'architect':
                await (0, architect_1.run)();
                break;
            case 'worker':
                await (0, worker_1.run)();
                break;
            case 'reviewer':
                await (0, reviewer_1.run)();
                break;
            default:
                throw new Error(`Unknown role: ${role}`);
        }
    }
    catch (error) {
        const err = error;
        console.error(err);
        core.setFailed(err.message);
    }
}
run();
