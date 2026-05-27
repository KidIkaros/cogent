#!/usr/bin/env python3
"""Generate synthetic code fixtures for Cogent benchmarks.

Usage:
    python generate.py [small|medium|large]

Creates mixed-language codebases of configurable size for performance testing.
"""

import argparse
import os
import random
import string

RUST_FUNCTION = '''
/// {doc}
pub fn {name}({params}) -> {ret} {{
    {body}
}}
'''

PYTHON_FUNCTION = '''
def {name}({params}):
    """{doc}"""
    {body}
'''

JS_FUNCTION = '''
/**
 * {doc}
 */
function {name}({params}) {{
    {body}
}}
'''

RUST_BODY_SIMPLE = [
    "let result = a + b;\n    result * 2",
    "vec![1, 2, 3].iter().sum()",
    "if x > 0 { x } else { -x }",
    "match value { Some(v) => v, None => 0 }",
]

PYTHON_BODY_SIMPLE = [
    "return a + b",
    "return [x for x in items if x > 0]",
    "if x > 0:\n        return x\n    return -x",
    "return value if value is not None else 0",
]

JS_BODY_SIMPLE = [
    "return a + b;",
    "return items.filter(x => x > 0);",
    "return x > 0 ? x : -x;",
    "return value ?? 0;",
]

TODO_MARKERS = [
    "// TODO: refactor this",
    "// FIXME: handle edge case",
    "// HACK: workaround for bug",
    "# TODO: add validation",
    "# FIXME: optimize this",
]


def random_name(length=8):
    return ''.join(random.choices(string.ascii_lowercase, k=length))


def generate_rust_file(path, n_functions=10, include_issues=False):
    lines = ['pub mod generated;\n']
    for i in range(n_functions):
        doc = f"Function {i} generated for benchmarking"
        name = f"func_{random_name(6)}"
        params = "a: i32, b: i32"
        ret = "i32"
        body = random.choice(RUST_BODY_SIMPLE)
        if include_issues and random.random() < 0.1:
            body += "\n    " + random.choice(TODO_MARKERS)
        func = RUST_FUNCTION.format(doc=doc, name=name, params=params, ret=ret, body=body)
        lines.append(func)
    with open(path, 'w') as f:
        f.write('\n'.join(lines))


def generate_python_file(path, n_functions=10, include_issues=False):
    lines = ['# Generated benchmark fixture\n']
    for i in range(n_functions):
        doc = f"Function {i}"
        name = f"func_{random_name(6)}"
        params = "a, b"
        body = random.choice(PYTHON_BODY_SIMPLE)
        if include_issues and random.random() < 0.1:
            body += "\n    " + random.choice(TODO_MARKERS)
        func = PYTHON_FUNCTION.format(doc=doc, name=name, params=params, body=body)
        lines.append(func)
    with open(path, 'w') as f:
        f.write('\n'.join(lines))


def generate_js_file(path, n_functions=10, include_issues=False):
    lines = ['// Generated benchmark fixture\n']
    for i in range(n_functions):
        doc = f"Function {i}"
        name = f"func{random_name(6)}"
        params = "a, b"
        body = random.choice(JS_BODY_SIMPLE)
        if include_issues and random.random() < 0.1:
            body += "\n    " + random.choice(TODO_MARKERS)
        func = JS_FUNCTION.format(doc=doc, name=name, params=params, body=body)
        lines.append(func)
    with open(path, 'w') as f:
        f.write('\n'.join(lines))


def generate_fixture_set(base_dir, n_files, funcs_per_file, include_issues=False):
    os.makedirs(base_dir, exist_ok=True)
    for i in range(n_files):
        lang = random.choice(['rust', 'python', 'js'])
        if lang == 'rust':
            path = os.path.join(base_dir, f"module_{i:04d}.rs")
            generate_rust_file(path, funcs_per_file, include_issues)
        elif lang == 'python':
            path = os.path.join(base_dir, f"module_{i:04d}.py")
            generate_python_file(path, funcs_per_file, include_issues)
        else:
            path = os.path.join(base_dir, f"module_{i:04d}.js")
            generate_js_file(path, funcs_per_file, include_issues)
    print(f"Generated {n_files} files in {base_dir}")


def main():
    parser = argparse.ArgumentParser(description='Generate benchmark fixtures')
    parser.add_argument('size', choices=['small', 'medium', 'large'],
                        default='small', nargs='?')
    parser.add_argument('--include-issues', action='store_true',
                        help='Add TODO/FIXME markers for debt scanning')
    args = parser.parse_args()

    script_dir = os.path.dirname(os.path.abspath(__file__))

    configs = {
        'small': (50, 10),    # 50 files, 10 funcs each = ~500 funcs
        'medium': (500, 20),  # 500 files, 20 funcs each = ~10k funcs
        'large': (2000, 50),  # 2000 files, 50 funcs each = ~100k funcs
    }

    n_files, funcs_per_file = configs[args.size]
    output_dir = os.path.join(script_dir, args.size)

    print(f"Generating {args.size} fixture: {n_files} files x {funcs_per_file} functions")
    generate_fixture_set(output_dir, n_files, funcs_per_file, args.include_issues)
    print(f"Done. Output: {output_dir}")


if __name__ == '__main__':
    main()
