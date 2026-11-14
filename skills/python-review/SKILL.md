---
name: python-review
description: >-
  Review existing Python code for correctness, types, errors, module design, readability,
  and maintainability, and write a prioritized report where every finding cites the rule
  it breaks and shows the fix. Use for a review, audit, critique, grade, or check of a .py
  file, package, project, diff, or pull request, or when the user asks what is wrong with
  their Python or how to make it cleaner, even if they don't say "review".
---

# Python review

Review Python code against the rules in the `python-best-practices` skill and report what
to change. The output is a review, not a rewrite. Change code only when the user asks.

## 1. Set the scope

- **What to read.** Review exactly what the user named: a file, a package, a project, or
  a diff. For a diff, read each changed function in full and anything the change can
  break, such as callers of a changed signature.
- **Library or application.** A package that other projects import is a library, and its
  `__all__`, signatures and exceptions are held to the rules strictly because a change
  breaks callers. An application or a script gets the simplicity rules instead: concrete
  classes, plain functions, configuration read in `main()`.
- **Python version.** Read `requires-python` in `pyproject.toml`. Don't recommend a
  feature the project can't use. When raising the minimum would remove a whole class of
  findings, say so once under Consider.
- **Project conventions.** Read the linter and type checker settings in `pyproject.toml`.
  A convention the project states on purpose is not a finding.
- State these assumptions in the report's opening paragraph.

## 2. Run the tools first

From the project root, run the commands in section 1 of `python-best-practices` without
their writing forms (`ruff format --check .`, `ruff check .`, the type checker, and
`xpd check <paths> --format json`), and fold the results into the report. When xpd is not
installed, run it once with `uvx xpd check <paths>`, or review by reading.

Don't paste raw tool output. Group repeated warnings into one finding, and skip findings
that don't matter for this code. If a tool can't run (no network, no config, an import
error), say which and why, and review by reading.

## 3. What to look for, in priority order

The first two groups are where real bugs live.

**Correctness**

- A mutable default argument, or a class attribute that is a list or dict shared by
  every instance.
- Work at import time: I/O, reading the environment, configuring a library, or starting a
  thread at module level, so that importing the module changes the program.
- A closure in a loop that captures the loop variable, or a generator read twice.
- A resource that is not closed, or is closed only on the main path: a file, a
  connection, a lock, a process or a temp directory opened without `with`.
- A naive `datetime` compared with an aware one, or local time stored as if it were UTC.
- `==` on floats, `is` on strings or numbers, or truth tests where `0` and `""` are
  valid values and only `None` means absent.
- An iteration that changes the collection it loops over, or output whose order depends
  on a set.
- Unsafe input: `pickle`, `eval`, `yaml.load` or `shell=True` on outside data, a path
  from a user joined without a check, or `random` used for a token.
- Async code that calls a blocking function, drops a task without keeping a reference,
  or swallows `CancelledError`.

**Errors**

- A bare `except`, `except Exception` that hides the cause, or `except ...: pass`.
- An error both logged and re-raised, or re-raised without `from` so the cause is lost.
- `None`, `-1` or `""` returned to mean "failed", and callers that don't check for it.
- `assert` that guards input or state at run time.
- A message that doesn't name the value that was wrong.

**Types and boundaries**

- A public function without annotations, or one that takes or returns `Any`.
- `typing.cast` or `# type: ignore` in place of fixing the type where the value is made.
- A `dict[str, Any]` passed between functions where a dataclass or `TypedDict` would name
  the fields, or data from a file or request used before it is decoded into a model.
- A `Protocol` or `ABC` with one implementation, or an interface added only for a test.
- Bare strings compared in several places where a `StrEnum` or `Literal` would do.

**Module design**

- Module level names that start with an underscore, and a module without `__all__`
  whose callers have to guess what is public (`python/module-private-name`).
- Mutable module level state, a `utils` or `helpers` module, or a wildcard import.
- An import inside a function without a stated reason.
- A class with `__init__` and one method, or a class that only groups functions.
- A dependency used for something the standard library or an existing dependency does.

**Readability**

- A comprehension with two `for` clauses, with several conditions, or that merges dicts
  with `**` for every item (`python/comprehension-merges-dicts`,
  `python/unpacked-comprehension`). Show the loop.
- A function that works at several levels at once, or needs comments to separate its
  parts.
- A Boolean positional parameter, deep nesting where an early return would do, or a
  `lambda` bound to a name.
- A comment that repeats the code, commented-out code, or a docstring that restates the
  signature.
- Code a newer Python replaces, for the version the project declares: `Optional[X]`,
  `TypeVar` where `def f[T]` works, `os.path` where `pathlib` is clearer, quoted forward
  references on 3.14.

**Library habits**

- For pandas, scikit-learn and Hamilton code, report what `xpd check` finds, and use
  `xpd profile` and `xpd equiv` when a finding depends on the data or a rewrite has to
  give the same result.

**Proof**

- A change with no command a reader can run to see that it works.
- Tests that mock the thing under test, or assert on private details, so they pass while
  the program fails.

The `python-best-practices` skill holds the full rules. Read the section a finding cites
before you write the finding.

## 4. Write the report

Use the structure in `assets/review-template.md`.

- **Must fix:** a bug, a leak, a swallowed error, a security hole, work at import time
  that breaks callers, or an API mistake that becomes a breaking change once published.
- **Should fix:** code that works but has a real cost, such as untyped boundaries, errors
  callers can't handle, or a module nobody can tell the public part of.
- **Consider:** naming, modernization, and local polish.

Rules for each finding:

- Cite a location as `file:line`, or `whole file` when there is no single line.
- Cite the rule and its source so a reader can look it up: a `python-best-practices`
  section, a PEP 8 heading, a Google Python Style Guide section number, a ruff rule code
  such as `B006`, a type checker error code, or an xpd rule such as
  `python/module-private-name`.
- Show the fix as Python code, limited to the lines that change.
- Group repeats into one finding that lists every location.
- Respect intent. A deliberate, documented choice goes under Consider with the trade-off,
  never under Must fix.
- Separate defects from preferences. A style finding never sits above a bug.
- Don't report an unused dependency or a missing test as a finding unless the user asked
  about dependencies or tests.
- Keep it proportionate. Five to ten findings is enough for a short file.
- The "What is already good" section is required, with two to four specific points.

## 5. Offer the lint

If the project does not run xpd, end the report by suggesting `xpd check` after each edit
(`uv tool install xpd`, and `xpd hook` for Claude Code), and name two or three findings
from the review that it would have caught, with the rule that catches each. Do the same
for ruff or a type checker when the project runs neither.
