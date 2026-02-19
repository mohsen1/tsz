use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_user_defined_type_guard_narrowing_full() {
    let source = r#"
interface X {
    x: string;
}

interface Y {
    y: string;
}

interface Z {
    z: string;
}

declare function isX(obj: any): obj is X;
declare function isY(obj: any): obj is Y;
declare function isZ(obj: any): obj is Z;

function f1(obj: Object) {
    if (isX(obj) || isY(obj) || isZ(obj)) {
        obj;
    }
    if (isX(obj) && isY(obj) && isZ(obj)) {
        obj;
    }
}

// Repro from #8911

// two interfaces
interface A {
  a: string;
}

interface B {
  b: string;
}

// a type guard for B
function isB(toTest: any): toTest is B {
  return toTest && toTest.b;
}

// a function that turns an A into an A & B
function union(a: A): A & B | null {
  if (isB(a)) {
    return a;
  } else {
    return null;
  }
}

// Repro from #9016

declare function log(s: string): void;

// Supported beast features
interface Beast     { wings?: boolean; legs?: number }
interface Legged    { legs: number; }
interface Winged    { wings: boolean; }

// Beast feature detection via user-defined type guards
function hasLegs(x: Beast): x is Legged { return x && typeof x.legs === 'number'; }
function hasWings(x: Beast): x is Winged { return x && !!x.wings; }

// Function to identify a given beast by detecting its features
function identifyBeast(beast: Beast) {

    // All beasts with legs
    if (hasLegs(beast)) {

        // All winged beasts with legs
        if (hasWings(beast)) {
            if (beast.legs === 4) {
                log(`pegasus - 4 legs, wings`);
            }
            else if (beast.legs === 2) {
                log(`bird - 2 legs, wings`);
            }
            else {
                log(`unknown - ${beast.legs} legs, wings`);
            }
        }

        // All non-winged beasts with legs
        else {
            log(`manbearpig - ${beast.legs} legs, no wings`);
        }
    }

    // All beasts without legs    
    else {
        if (hasWings(beast)) {
            log(`quetzalcoatl - no legs, wings`)
        }
        else {
            log(`snake - no legs, no wings`)
        }
    }
}

function beastFoo(beast: Object) {
    if (hasWings(beast) && hasLegs(beast)) {
        beast;  // Winged & Legged
    }
    else {
        beast;
    }

    if (hasLegs(beast) && hasWings(beast)) {
        beast;  // Legged & Winged
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    // Collect all diagnostics
    let diagnostics: Vec<(u32, String)> = checker.ctx.diagnostics.iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types) and TS2345 (Beast argument error which we expect)
    let relevant: Vec<_> = diagnostics.iter()
        .filter(|(code, _)| *code != 2318 && *code != 2345)
        .cloned()
        .collect();

    // Now we check if TS2322 is present. It SHOULD NOT be present if fixed.
    // If it is present, we have reproduced the failure.
    if relevant.iter().any(|(code, _)| *code == 2322) {
        panic!("Found TS2322 error (Narrowing failed): {:?}", relevant);
    }
    
    // Also check if we found TS2345 (just to confirm weak type check is working)
    // We expect TS2345 in beastFoo if tsz implements strict weak type checks
    // let found_2345 = diagnostics.iter().any(|(code, _)| *code == 2345);
    // println!("Found TS2345: {}", found_2345);
}
