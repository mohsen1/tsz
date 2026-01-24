#!/usr/bin/env python3
import re
from pathlib import Path

def count_function_lines(file_path):
    """Count lines in each function and return the largest ones."""
    with open(file_path, 'r') as f:
        lines = f.readlines()

    functions = []
    current_function = None
    current_start = 0
    brace_count = 0
    in_function = False

    for i, line in enumerate(lines):
        # Look for function definitions
        if re.match(r'^\s*(?:pub\s*)?(?:crate\s*)?(?:async\s*)?fn\s+\w+', line):
            in_function = True
            current_start = i
            brace_count = 0
            # Extract function name
            match = re.search(r'fn\s+(\w+)', line)
            if match:
                current_function = match.group(1)

        # Count braces to determine function scope
        if in_function:
            brace_count += line.count('{') - line.count('}')

            # Function ends when braces return to 0
            if brace_count == 0 and i > current_start:
                functions.append((current_function, i - current_start + 1, current_start + 1))
                in_function = False
                current_function = None

    return sorted(functions, key=lambda x: x[1], reverse=True)

def analyze_directory(source_dir):
    """Analyze all Rust files in the source directory."""
    results = []

    for rs_file in Path(source_dir).rglob('*.rs'):
        # Skip test files
        if '_tests.rs' in rs_file.name:
            continue

        try:
            functions = count_function_lines(rs_file)
            if functions and functions[0][1] > 50:  # Only show functions > 50 lines
                results.append((rs_file, functions[:10]))  # Top 10 per file
        except Exception as e:
            print(f"Error processing {rs_file}: {e}")

    # Sort all functions by size
    all_functions = []
    for file_path, funcs in results:
        for func_name, line_count, line_num in funcs:
            all_functions.append((file_path.name, func_name, line_count, line_num))

    all_functions.sort(key=lambda x: x[2], reverse=True)

    return all_functions[:50]  # Top 50 overall

if __name__ == '__main__':
    source_dir = '/home/mohsen_thirdface_com/oc/tsz-workspace/worktrees/worker-14/src'

    print("=" * 80)
    print("LARGEST FUNCTIONS IN CODEBASE")
    print("=" * 80)
    print(f"{'Function':<50} {'File':<30} {'Lines':>10}")
    print("-" * 90)

    results = analyze_directory(source_dir)

    for file_name, func_name, line_count, line_num in results:
        print(f"{func_name:<50} {file_name:<30} {line_count:>10}")
