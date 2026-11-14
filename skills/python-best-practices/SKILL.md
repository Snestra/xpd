---
name: python-best-practices
description: >-
  Write clear, typed, maintainable Python. Use whenever you write, change, refactor, debug,
  or explain Python code: modules, functions, classes, type hints, dataclasses, errors,
  comprehensions, imports, pyproject.toml, or scripts. Trigger on Python, .py files, uv,
  ruff, type checkers, pandas, or notebooks turned into modules, even without the words
  "best practices".
---

# Python best practices

Follow PEP 8, the Google Python Style Guide and the typing documentation. This file holds
the rules where they leave a choice open, where models drift, and what changed through
Python 3.15. Where a project already has a convention, the project wins.

## 1. Before you hand back code

From the project root, run what the project has configured:

```bash
ruff format .           # or the project's formatter
ruff check .
ty check                # or the project's type checker: pyright, mypy
xpd check .             # when xpd is installed: the lint for sections 3 and 4
```

- Fix a finding rather than silencing it. To suppress one line, name the rule and the
  reason: `# noqa: S603  the arguments are a fixed list`.
- Then run the real command or app on real input and read its output. A clean lint run
  shows the code is tidy, not that it works.

## 2. Versions

Read `requires-python` in `pyproject.toml` before you use a feature below. A new project
declares the current release (`>=3.15`). A library declares the oldest release it tests.
Never take the minimum from `.python-version` or from the interpreter that happens to run.

| Since | Use | Instead of |
|---|---|---|
| 3.10 | `X \| None`, `match`, `zip(strict=True)`, `kw_only=True` and `slots=True` on dataclasses | `Optional[X]`, `Union`, long `isinstance` chains |
| 3.11 | `Self`, `tomllib`, `StrEnum`, `asyncio.TaskGroup`, `except*`, `err.add_note()` | a `TypeVar` bound to the class, `tomli`, `asyncio.gather` |
| 3.12 | `def first[T](xs: list[T]) -> T`, `type Pair = tuple[int, int]`, `@override`, `itertools.batched`, `Path.walk` | `TypeVar`, `TypeAlias`, `os.walk` |
| 3.13 | type parameter defaults, `@warnings.deprecated`, `copy.replace`, `ReadOnly`, `TypeIs` | `dataclasses.replace` for other types, `TypeGuard` for narrowing |
| 3.14 | annotations evaluated on demand, template strings (`t"..."`), `compression.zstd`, `concurrent.interpreters` | quoted forward references, `from __future__ import annotations` |
| 3.15 | `lazy import`, `frozendict`, the `sentinel` type, `{**d for d in dicts}` and `[*xs for xs in lists]` | imports inside functions to save startup time, `object()` sentinels, `ChainMap` or a loop to merge dicts |

Check the dependencies and the type checker separately. A language feature the interpreter
accepts can still be one the checker or a library does not support.

## 3. Names and modules

- **Module level names are public.** Don't start a module level function, class or
  variable with an underscore. List the names callers may import in `__all__`, at the top
  of the module, and leave the rest out of it:

```python
__all__ = ["total"]

LIMIT = 100


def prices(rows: list[Row]) -> list[float]:
    return [row.price for row in rows][:LIMIT]


def total(rows: list[Row]) -> float:
    return sum(prices(rows))
```

- **A method or attribute of a class may be private.** `def _load(self)` is fine, and so
  is a name inside a function.
- A project that uses a framework where the underscore has a meaning keeps it. Hamilton
  reads `_helper` as "not a node".
- **Importing a module does nothing.** No I/O, no configuration of a library, no reading
  of the environment and no logging setup at module level. Put it in a function that
  `main()` or the caller runs.
- No mutable module level state. A cache belongs to an object, or behind
  `functools.cache` on a function.
- Name a module after what it holds. No `utils`, `helpers`, `common` or `misc`.
- Imports sit at the top of the file, absolute, one module per line. No wildcard
  imports. An import inside a function needs a comment with its reason, and on 3.15
  `lazy import` replaces most of them.
- Constants are `UPPER_CASE`, classes `CapWords`, everything else `lower_case`. No type
  in the name (`user_list`, `name_str`), and no single letters outside a short loop or
  comprehension.

## 4. Functions and control flow

- **Return early.** Keep the main path at the left margin, with no `else` after `return`
  or `raise`.
- **Booleans and options are keyword-only.** `def load(path, *, strict=False)`.
- **No mutable default.** Use `None` and build the value inside, or a tuple.
- **A comprehension maps or filters one value per item.** One `for`, at most one `if`,
  and an expression that fits on a line. Anything more is a loop.
- **Build a row or a record in a loop.** Don't merge dicts with `**` inside a
  comprehension, and don't unpack a comprehension inside a dict literal:

```python
def table(models: list[Model], catalog: Catalog) -> list[dict[str, object]]:
    rows = []
    for model in models:
        row: dict[str, object] = {"id": model.id}
        row.update(owner(model))
        row.update(limits(catalog, model))
        rows.append(row)
    return rows
```

- A generator expression goes straight into the call that consumes it, such as `sum`,
  `any` or `"".join`. When the caller may stop early or the data is large, write a
  generator function.
- No `lambda` bound to a name. Write `def`.
- A function does one thing at one level. When it needs a comment to separate its parts,
  split it into functions named after the parts.

## 5. Types

- **Annotate every parameter and return value.** Inside a function, annotate only what
  the checker cannot infer, such as an empty list.
- **Data that crosses a boundary gets a named model.** A file, a request, a response, a
  database row or a config file is decoded once into a dataclass, a `TypedDict` or the
  project's model library, and the rest of the code takes the model. No `dict[str, Any]`
  passed between functions.
- **No `typing.cast`.** Fix the type where the value is made, or narrow it with
  `isinstance`, `match` or a `TypeIs` function.
- **No `Any`** except at the edge of an untyped library, and then in one function that
  returns a typed value.
- `# type: ignore` names the error code and the reason, on that line only.
- **A concrete class or a plain function until a second implementation exists.** No
  `Protocol` or `ABC` for one implementation, and none added only for a test.
- Parameters take the widest type that works (`Iterable[str]`, `Mapping[str, int]` from
  `collections.abc`). Return values are concrete (`list[str]`).
- A value object is `@dataclass(frozen=True, slots=True, kw_only=True)`. A closed set of
  names is a `StrEnum` or `Literal`, never bare strings compared in several places.
- A class that only has `__init__` and one method is a function.

## 6. Errors

- **Check with `if` and `raise`** the most specific built-in exception, or one class the
  project defines for callers to catch. The message says what was wrong and names the
  value: `raise ValueError(f"unknown region: {region!r}")`.
- **Catch only what you can handle,** as narrowly as the call allows. No bare `except`,
  and no `except Exception` outside the top of a program or a worker loop.
- **Handle an error once.** Re-raise it with context, or log it and stop, never both.
  `raise ConfigError(f"cannot read {path}") from err` keeps the cause.
- No `None`, `-1` or `""` returned to mean "failed". Raise, or return `X | None` only when
  absence is a normal answer.
- No `assert` for input or state that can be wrong at run time. `python -O` removes it.
- No `try` around an import to make a dependency optional, unless the project declares
  it as an extra.

## 7. Data, I/O and boundaries

- `pathlib.Path` for paths. Open every file, socket and connection with `with`.
- Pass `encoding="utf-8"` when a project supports a release below 3.15.
- `subprocess.run([...], check=True)` with a list, a timeout when the child can hang, and
  never `shell=True` with a value from outside.
- Instants are timezone-aware `datetime` values (`datetime.now(UTC)`), and spans are
  `timedelta`. A bare number carries its unit in the name (`timeout_s`).
- Read configuration and the environment once, in `main()`, and pass values down.
- `secrets` for tokens, `random` for the rest. Never `pickle`, `eval` or `yaml.load` on
  data from outside.
- Log with `logging.getLogger(__name__)` and lazy arguments
  (`log.info("loaded %d rows", n)`). A library never configures logging and never prints.

## 8. Comments and docstrings

- **A comment gives a reason the code cannot show**: why this order, why this constant,
  which bug in a dependency it avoids. Delete a comment that says what the next line
  does, and rename until it isn't needed.
- No commented-out code, no banners, no change history, no TODO without an owner.
- **A docstring is one line** that says what the caller gets, in the imperative
  (`"""Return the rows that changed since the last sync."""`). Add more only for what the
  signature cannot say: the exceptions callers should catch, units, side effects.
  Don't repeat the parameter names and types.

## 9. Proof that it works

- Run the real command or app with real data and show the output. For a bug, run the
  same case before and after the fix.
- Don't write unit tests, integration tests or tests that use mocks unless the project
  or the user asks for them. Write an end to end test when asked, and end it with an
  artifact someone can check and rerun.
- Never add an interface, a parameter or a module level hook only so that a test can
  replace it.

## 10. Projects

- `pyproject.toml` with a `[project]` table, a `src/<package>/` layout and a lockfile.
  Use the project's workflow to add a dependency (`uv add`), and ask before you add one.
- Check the standard library and the dependencies already there before you write a
  helper.
- A command is a `main()` function behind `[project.scripts]`. A script that is run
  directly ends with `if __name__ == "__main__": main()`.
- Tool settings live in `pyproject.toml`. Don't loosen the linter or the type checker
  to make a change pass.

## What a tool checks

| Rule | Tool |
|---|---|
| Layout, quotes, import order, unused names, mutable defaults, bare `except` | `ruff format`, `ruff check` |
| Types, `None` handling, missing annotations in strict mode | the project's type checker |
| Module level names with an underscore | `xpd check`: `python/module-private-name` |
| `**` merges and unpacked comprehensions | `xpd check`: `python/comprehension-merges-dicts`, `python/unpacked-comprehension` |
| Slow or wrong pandas, scikit-learn and Hamilton code | `xpd check`, in files that import them |

The rest of this file needs a reader. The `python-review` skill is that reader.

## Look up the rest

- Layout, naming and whitespace: [PEP 8](https://peps.python.org/pep-0008/)
- Comprehensions (2.7), exceptions (2.4), global state (2.5), default arguments (2.12),
  docstrings (3.8), naming (3.16), type annotations (3.19):
  [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html)
- Annotations, generics, narrowing, protocols:
  [typing documentation](https://typing.python.org/en/latest/)
- What each release added: [What's New in Python](https://docs.python.org/3/whatsnew/)
- Every xpd rule with before and after code:
  [xpd rules](https://bjoernaagaard.github.io/xpd/rules/)
