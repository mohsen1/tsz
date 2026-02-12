#!/bin/bash

echo "═══════════════════════════════════════════════════════════════════════"
echo "⚠️  ⚠️  ⚠️  YOUR SLICE MUST BE 100% PASSING - NO EXCEPTIONS ⚠️  ⚠️  ⚠️"
echo "═══════════════════════════════════════════════════════════════════════"
echo ""
echo "You are SLICE 3 of 4"

case 3 in
  1) echo "Your test range: offset 0, max 3146" ;;
  2) echo "Your test range: offset 3146, max 3146" ;;
  3) echo "Your test range: offset 6292, max 3146" ;;
  4) echo "Your test range: offset 9438, max 3145" ;;
esac

echo ""
echo "🚨 CRITICAL: PUSH EVERY COMMIT IMMEDIATELY 🚨"
echo "After EVERY commit, run: git push"
echo "Check it's synced: git log origin/main..HEAD (should be empty)"
echo ""
echo "TO VERIFY YOUR SLICE IS 100% PASSING:"

case 3 in
  1) echo "  ./scripts/conformance.sh run --offset 0 --max 3146" ;;
  2) echo "  ./scripts/conformance.sh run --offset 3146 --max 3146" ;;
  3) echo "  ./scripts/conformance.sh run --offset 6292 --max 3146" ;;
  4) echo "  ./scripts/conformance.sh run --offset 9438 --max 3145" ;;
esac

echo ""
echo "WORKFLOW:"
echo "  1. Analyze failures in YOUR slice"
echo "  2. Fix issues"
echo "  3. Run: cargo nextest run --release"
echo "  4. If tests pass, commit"
echo "  5. IMMEDIATELY: git push"
echo "  6. VERIFY: git log origin/main..HEAD (must be empty)"
echo "  7. Repeat until YOUR slice is 100%"
echo ""
echo "═══════════════════════════════════════════════════════════════════════"
echo "🚨 NEVER LEAVE UNPUSHED COMMITS 🚨"
echo "🚨 NEVER CLAIM SUCCESS WITHOUT 100% PASS VERIFICATION 🚨"
echo "═══════════════════════════════════════════════════════════════════════"
