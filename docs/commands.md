# Commands

`xpd -h` and `xpd <command> -h` always match the installed version.

## check

```sh
xpd check [PATHS]... [--format text|json] [--fix] [--unsafe-fixes]
```

Lints files and folders. Folders respect `.gitignore`. A library's rules run only on files
that import it, and the Python rules run on every file. A rule for a newer library version than the one installed in the project's venv
is skipped. `check` also warns once per run about a package the project's venv lacks, such as
pyarrow. See [Rules](rules.md).

Some rules have a fix, which is safe or unsafe. A safe fix keeps what the code does. An unsafe
fix may change it, for example because another name still points at the old object.

| Flags | Effect |
| --- | --- |
| none | Lint. `[*]` marks an issue with a safe fix |
| `--fix` | Write the safe fixes, then lint what is left |
| `--unsafe-fixes` | Lint. `[*]` also marks an issue with an unsafe fix |
| `--fix --unsafe-fixes` | Write every fix, then lint what is left |

Output is text on a terminal and JSON otherwise. The JSON has `fixed`, the number of fixes
written, and `issues`, each with `package`, `rule`, `file`, `line`, `column`, `severity`,
`message` and `fix`: `safe`, `unsafe` or `null`. A package warning is about the project and not
about a place in a file, so its `line` and `column` are `null`.

| Exit code | Meaning |
| --- | --- |
| 0 | No error-severity issue, after fixes |
| 1 | At least one error |
| 2 | xpd could not run, for example an unreadable path |

## profile

```sh
xpd profile data.csv
```

Prints JSON with each column's dtype, null count, unique count and memory use. Reads csv,
parquet, feather, json and xlsx. Runs with the project's Python and needs pandas there.

## equiv

```sh
xpd equiv old.py new.py
```

Runs both scripts and compares the variable `result` each one leaves. DataFrames and Series
are compared with `pandas.testing`, anything else with `numpy.testing`, so a list and an
array with the same values count as equal. Run it after you refactor data logic.

| Exit code | Meaning |
| --- | --- |
| 0 | `{"equivalent": true}` |
| 1 | The results differ, with the difference in `detail` |
| 2 | A script failed, has no `result`, or pandas is missing |

## graph

```sh
xpd graph [PATHS]... [--format text|json]
```

Prints the dataflow of Python files: which part of the code reads the result of which other
part, and what does not connect. It imports nothing and runs nothing, so it needs no Python.
It reads two ways of writing a dataflow, and picks one per file.

| A file with | Is read as |
| --- | --- |
| A function that passes a frame through other functions of the file | [Step code](#step-code) |
| Anything else | [Hamilton modules](#hamilton-modules) |

A file with a pipeline that also imports `hamilton` is read as both.

### Step code

A step is a function that takes a frame and returns it with columns added. A pipeline is a
function that runs steps in order, as `frame.pipe(a).pipe(b)` or as `frame = a(frame)` followed
by `frame = b(frame)`. `graph` reads from the source which columns each step reads and adds,
so it knows which step needs which.

```console
$ xpd graph samples/graph/steps.py
samples/graph/steps.py:51: features: 4 steps, 7 columns
samples/graph/steps.py:18:   (source columns) -> totals  [2 columns]
samples/graph/steps.py:28:   totals -> shares  [1 column]
samples/graph/steps.py:33:   (source columns) -> discounts  [1 column]
samples/graph/steps.py:41:   totals + discounts -> flags  [3 columns]
samples/graph/steps.py:35: warning: discounts() computes before_discount with the formula of the column gross_amount from totals(). Read the column gross_amount there.
samples/graph/steps.py:37: warning: discounts() writes to the frame it was given. Called as discounts(orders), it changes the caller's frame too. Return orders.assign(...) or write to orders.copy().
samples/graph/steps.py:45: warning: The column over_limit from flags() has the same formula as is_large from flags(), so the result holds one column twice. Keep one of them.
samples/graph/steps.py:51: warning: The tree in the module docstring lists totals, shares, flags and features() runs totals, shares, discounts, flags. Change the one that is out of date.
xpd: 1 pipeline with 4 steps in 1 file, 4 warnings
```

The first line names the pipeline. Each line after it is `file:line: steps it reads from ->
step`, with the number of columns the step adds. `order is free for` lists the steps that share
no column with another step. Then come the findings:

| Finding | Severity | What happens |
| --- | --- | --- |
| A step reads a column that a later step joins | error | The read raises `KeyError` |
| A step joins a column that an earlier step added | error | The join raises "columns overlap" |
| A step computes the formula of a column that another step adds | warning | The same work is done twice, and the two can drift apart |
| Two columns have the same formula | warning | The result holds one column twice |
| Two steps write out the same formula | warning | As above, with no column to read it from |
| A column is assigned twice in one step with the same formula | warning | The first assignment is wasted |
| A step writes to the frame it was given, as `frame["x"] = ...` | warning | Called as `step(frame)`, it changes the caller's frame |
| The tree in the module docstring differs from the pipeline | warning | The description is out of date |
| A frame of new columns is not joined to the result | note | The result has no column of those names |

Two formulas are the same when they are equal after each local name is replaced by what it
stands for and the terms of a sum or a product are sorted. So `a + b` in one step equals `b + a`
in another.

What `graph` reads:

- A column read as `frame["name"]`, where the name is a literal, a constant of the module, or a
  list of those. `frame.name` counts when another step adds a column of that name.
- A column added with `frame.assign(name=...)`, with `frame.join(new)` where `new` is
  `pd.DataFrame(index=frame.index).assign(name=...)`, or with `frame["name"] = ...`.
- A step at module level. A method of a class is not read.

A step whose column names the file computes, as in `frame[prefix + "_total"]` or a join of
`pd.get_dummies(...)`, is marked `and computed names`. Its named columns are still read, and it
is left out of `order is free for`, because not all of its columns are known.

The tree check applies to a file with one pipeline whose module docstring draws its steps as
`├── step()` lines.

### Hamilton modules

Prints the dataflow of [Apache Hamilton](https://hamilton.apache.org/) modules. Hamilton turns
every function in a module into a node: the function name is the output and the parameter names
are its inputs. `graph` reads the files and prints which node reads which. It imports nothing
and runs nothing, so it needs no Python and no Hamilton.

Pass the modules you give to one Driver. Together they are one dataflow, as with
`with_modules(loaders, features)`. A folder means every `.py` file in it. A script that builds
the Driver is not a node module, and its functions would show as nodes.

```console
$ xpd graph samples/graph/features.py
samples/graph/features.py:10: (no input) -> uk
samples/graph/features.py:14: (no input) -> dk
samples/graph/features.py:18: uk + dk -> spread
samples/graph/features.py:22: spraed -> spread_sq
inputs: spraed
read by no node: spread, spread_sq
samples/graph/features.py:22: warning: spread_sq reads spraed, which no function defines and which is one edit from the node spread. If it is a typing error, Hamilton raises "Required input spraed not provided" at execute.
xpd: 4 nodes from 4 functions in 1 file, 1 input, 1 warning
```

Each node line is `file:line: inputs -> node`. An input with `?` has a default. `inputs` lists
the names that no function defines, which the caller has to pass. `read by no node` lists the
ends of the dataflow. Then come the findings:

| Finding | Severity | What happens in Hamilton |
| --- | --- | --- |
| A function name defined twice in one file | error | Python keeps the last `def`, so the others never become nodes. Two `@config.when` versions need the names `rate__uk` and `rate__dk` |
| A node defined by two functions | error | The Driver raises "Cannot define function ... more than once" when it is built |
| A plain `def rate` beside `@config.when` on `rate__dk` | error | The same error, once the config selects the version |
| No type hint on the return value or on a parameter | error | The Driver raises "Missing type hint" when it is built |
| An input one edit from a node name | warning | If it is a typing error, execute raises "Required input ... not provided" |
| A decorator that `graph` cannot read | note | Nothing. The graph may lack nodes |

`graph` reads `@config.when` and its relatives, `@extract_columns`, `@extract_fields`,
`@unpack_fields`, `@parameterize`, `@parameterize_values`, `@parameterize_sources`, `@inject`,
`@load_from` and `@save_to`, when their arguments are literals. It prints a note for names that
the file computes, such as `@extract_columns(*COLUMNS)`, and for `@subdag`, `@pipe_input`,
`@pipe_output`, `@mutate`, `@with_columns`, `@does` and `@resolve`. It does not know the config
the Driver gets, so it lists every version of a node. It does not compare type hints between
nodes.

### Output and exit code

Output is text on a terminal and JSON otherwise. The JSON has `pipelines`, each with `name`,
`file`, `line`, `steps`, `links` and `free`; `nodes`, each with `name`, `file`, `line`,
`function`, `inputs`, `kind` and `config`; `inputs`, each with `name`, `optional` and `readers`;
`unread`; and `problems`, each with `file`, `line`, `severity` and `message`. A step has `name`,
`line`, `reads`, `columns` and `computed`. A link has `from`, `to` and `columns`.

| Exit code | Meaning |
| --- | --- |
| 0 | No error |
| 1 | At least one error |
| 2 | xpd could not run, for example an unreadable path |

## hook

Reads a Claude Code hook event on stdin and lints the `.py` file the edit touched. When the file
holds a pipeline of steps, the hook also prints the errors and warnings of `xpd graph` for it. See
[Claude Code](claude-code.md).

## skill

Prints the agent skill that ships with this version of xpd.
