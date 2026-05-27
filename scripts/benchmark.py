#!/usr/bin/env python3
"""Benchmark cogent against a synthetic large repo."""
import os
import sys
import shutil
import random
import time
import subprocess
import tempfile

REPO_DIR = tempfile.mkdtemp(prefix="qt-bench-")

# Code templates per language — varying complexity
RUST_FN = """pub fn {name}(x: i32, y: i32) -> i32 {{
    if x > 0 {{
        if y > 0 {{
            return x + y;
        }}
        return x - y;
    }}
    match y {{
        0 => 0,
        _ => x * y,
    }}
}}
"""

RUST_STRUCT = """/// A data structure.
pub struct {name} {{
    field_a: i32,
    field_b: String,
}}

impl {name} {{
    pub fn new() -> Self {{
        Self {{ field_a: 0, field_b: String::new() }}
    }}

    pub fn compute(&self, input: i32) -> i32 {{
        if input > 10 {{
            return self.field_a + input;
        }}
        self.field_a - input
    }}
}}
"""

PYTHON_FN = """def {name}(x, y):
    if x > 0:
        if y > 0:
            return x + y
        return x - y
    if y == 0:
        return 0
    return x * y

class {cls}:
    def __init__(self):
        self.field_a = 0
        self.field_b = ""

    def compute(self, input_val):
        if input_val > 10:
            return self.field_a + input_val
        return self.field_a - input_val
"""

JS_FN = """function {name}(x, y) {{
    if (x > 0) {{
        if (y > 0) {{
            return x + y;
        }}
        return x - y;
    }}
    switch (y) {{
        case 0: return 0;
        default: return x * y;
    }}
}}

class {cls} {{
    constructor() {{
        this.fieldA = 0;
        this.fieldB = "";
    }}

    compute(input) {{
        if (input > 10) {{
            return this.fieldA + input;
        }}
        return this.fieldA - input;
    }}
}}
"""

TS_FN = """function {name}(x: number, y: number): number {{
    if (x > 0) {{
        if (y > 0) {{
            return x + y;
        }}
        return x - y;
    }}
    switch (y) {{
        case 0: return 0;
        default: return x * y;
    }}
}}

interface I{cls} {{
    fieldA: number;
    fieldB: string;
}}

class {cls} implements I{cls} {{
    fieldA = 0;
    fieldB = "";

    compute(input: number): number {{
        if (input > 10) {{
            return this.fieldA + input;
        }}
        return this.fieldA - input;
    }}
}}
"""

GO_FN = """package {pkg}

func {name}(x int, y int) int {{
    if x > 0 {{
        if y > 0 {{
            return x + y
        }}
        return x - y
    }}
    if y == 0 {{
        return 0
    }}
    return x * y
}}

type {cls} struct {{
    FieldA int
    FieldB string
}}

func (s *{cls}) Compute(input int) int {{
    if input > 10 {{
        return s.FieldA + input
    }}
    return s.FieldA - input
}}
"""

def generate_repo():
    """Generate a synthetic mixed-language repo."""
    counts = {"rs": 200, "py": 200, "js": 200, "ts": 200, "go": 200}
    for ext, count in counts.items():
        for i in range(count):
            if ext == "rs":
                code = RUST_FN.format(name=f"fn_{ext}_{i}")
                if i % 3 == 0:
                    code += RUST_STRUCT.format(name=f"Struct{ext}{i}")
            elif ext == "py":
                code = PYTHON_FN.format(name=f"fn_{ext}_{i}", cls=f"Cls{ext}{i}")
            elif ext == "js":
                code = JS_FN.format(name=f"fn_{ext}_{i}", cls=f"Cls{ext}{i}")
            elif ext == "ts":
                code = TS_FN.format(name=f"fn_{ext}_{i}", cls=f"Cls{ext}{i}")
            elif ext == "go":
                code = GO_FN.format(name=f"fn_{ext}_{i}", cls=f"Cls{ext}{i}", pkg=f"pkg{i}")
            path = os.path.join(REPO_DIR, f"src_{ext}", f"file_{i}.{ext}")
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w") as f:
                f.write(code)
    total = sum(counts.values())
    print(f"Generated {total} files in {REPO_DIR}")
    return total

def run_tool(tool, args, cwd):
    """Run a tool and return elapsed seconds."""
    start = time.perf_counter()
    proc = subprocess.run(
        [tool] + args,
        cwd=cwd,
        capture_output=True,
        text=True,
    )
    elapsed = time.perf_counter() - start
    return elapsed, proc.returncode, proc.stderr[:200]

def benchmark():
    total = generate_repo()
    tools = [
        ("debt", [".", "--recursive"]),
        ("doccov", [".", "--recursive"]),
        ("crap", [".", "--recursive"]),
        ("dupfind", [".", "--recursive"]),
        ("taint", [".", "--recursive"]),
        ("riskmap", ["."]),
        ("coupling", ["."]),
        ("fuzz", [".", "--recursive"]),
        ("propcov", [".", "--recursive"]),
    ]

    print(f"\n{'Tool':12s} {'Time(s)':>10s} {'Status':>8s}")
    print("-" * 35)
    for bin_name, args in tools:
        # Find binary
        binary = os.path.join(os.path.dirname(__file__), "..", "target", "release", bin_name)
        if not os.path.exists(binary):
            print(f"{bin_name:12s} {'---':>10s} (not built)")
            continue
        elapsed, rc, err = run_tool(binary, args, REPO_DIR)
        status = "OK" if rc == 0 else f"ERR({rc})"
        print(f"{bin_name:12s} {elapsed:10.3f} {status:>8s}")

    # Benchmark cogent run
    binary = os.path.join(os.path.dirname(__file__), "..", "target", "release", "quality")
    elapsed, rc, err = run_tool(binary, ["run", "."], REPO_DIR)
    status = "OK" if rc == 0 else f"ERR({rc})"
    print(f"{'quality':12s} {elapsed:10.3f} {status:>8s}")

    # Cleanup
    shutil.rmtree(REPO_DIR, ignore_errors=True)

if __name__ == "__main__":
    benchmark()
