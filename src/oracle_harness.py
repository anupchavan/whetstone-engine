"""Whetstone verification oracle harness.

Runs a model-authored verification script under containment and reports a
strict three-way verdict (architecture §9.2): proved | disproved | unsupported.

Containment model: the script text is derived from UNTRUSTED source material
(indirect prompt injection can reach it), so it never gets ambient authority.
Before execution the script is AST-checked against a whitelist: only sympy /
math-adjacent imports, no dunder access, no exec/eval/open/import machinery.
Execution gets restricted builtins, a CPU limit, a memory limit, and no argv.
This is containment for a local dev tool, not a hostile-multitenant sandbox;
the AST gate plus rlimits close the obvious escalations (imports, filesystem,
subprocess, unbounded loops).

Script contract:
  - runs top-to-bottom, recomputing the keyed answer from restated givens;
  - asserts the recomputed result matches the key (tolerances allowed);
  - clean completion -> proved; AssertionError -> disproved (the key is
    contradicted by computation); anything else -> unsupported.

Exit is always 0 with a one-line JSON verdict on stdout; the Rust caller
treats harness crashes as unsupported.
"""

import ast
import json
import resource
import sys

ALLOWED_IMPORTS = {
    "sympy", "math", "cmath", "fractions", "decimal", "itertools",
    "functools", "statistics",
}

FORBIDDEN_NAMES = {
    "eval", "exec", "compile", "open", "input", "__import__", "getattr",
    "setattr", "delattr", "globals", "locals", "vars", "breakpoint", "exit",
    "quit", "help", "memoryview", "classmethod", "staticmethod", "super",
    "object", "type",
}

def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
    """Runtime twin of the AST import gate: admit whitelisted modules only."""
    if level != 0 or name.split(".")[0] not in ALLOWED_IMPORTS:
        raise ImportError(f"import of '{name}' is outside the oracle contract")
    return __import__(name, globals, locals, fromlist, level)


SAFE_BUILTINS = {
    name: getattr(__builtins__, name) if hasattr(__builtins__, name) else __builtins__[name]
    for name in (
        "abs", "all", "any", "bool", "complex", "dict", "divmod", "enumerate",
        "filter", "float", "frozenset", "int", "isinstance", "issubclass",
        "iter", "len", "list", "map", "max", "min", "next", "pow", "print",
        "range", "repr", "reversed", "round", "set", "sorted", "str", "sum",
        "tuple", "zip", "Exception", "ValueError", "TypeError", "ArithmeticError",
        "ZeroDivisionError", "AssertionError", "StopIteration",
    )
}
SAFE_BUILTINS["__import__"] = guarded_import


def verdict(kind: str, detail: str) -> None:
    print(json.dumps({"verdict": kind, "detail": detail[:400]}))
    sys.exit(0)


def check_ast(tree: ast.AST) -> str:
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            module = getattr(node, "module", None)
            names = [module] if module else []
            names += [alias.name for alias in getattr(node, "names", [])]
            for name in names:
                if name and name.split(".")[0] not in ALLOWED_IMPORTS:
                    return f"import of '{name}' is outside the oracle contract"
        if isinstance(node, ast.Attribute) and node.attr.startswith("__"):
            return f"dunder attribute access '{node.attr}' is not allowed"
        if isinstance(node, ast.Name):
            if node.id in FORBIDDEN_NAMES or node.id.startswith("__"):
                return f"name '{node.id}' is not allowed"
        if isinstance(node, (ast.Global, ast.Nonlocal)):
            return "global/nonlocal declarations are not allowed"
    return ""


def main() -> None:
    source = sys.stdin.read()
    if len(source) > 20_000:
        verdict("unsupported", "verification script exceeds 20000 characters")
    try:
        tree = ast.parse(source)
    except SyntaxError as error:
        verdict("unsupported", f"syntax error: {error}")
    problem = check_ast(tree)
    if problem:
        verdict("unsupported", problem)

    resource.setrlimit(resource.RLIMIT_CPU, (10, 10))
    try:
        resource.setrlimit(resource.RLIMIT_AS, (1_000_000_000, 1_000_000_000))
    except (ValueError, OSError):
        pass  # macOS may reject RLIMIT_AS; the CPU limit still holds

    namespace = {"__builtins__": SAFE_BUILTINS, "__name__": "__oracle__"}
    try:
        exec(compile(tree, "<verification>", "exec"), namespace)  # noqa: S102 — gated by AST whitelist above
    except AssertionError as error:
        verdict("disproved", f"assertion failed: {error}")
    except BaseException as error:  # noqa: BLE001 — any other failure is non-evidence
        verdict("unsupported", f"{type(error).__name__}: {error}")
    verdict("proved", "verification script completed with all assertions passing")


if __name__ == "__main__":
    main()
