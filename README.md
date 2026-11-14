# xpd

xpd is a fast, opinionated linter for Python library habits that make code slow or wrong,
starting with pandas and scikit-learn. Each library gets one set of rules, written for its current
version. It fixes what it can, and it is built to run after every edit a coding agent makes.

```console
$ xpd check samples/bad.py
samples/bad.py:5:15: warning [pandas/row-loop] iterrows() builds a Series for every row. Compute on whole columns instead, such as df["a"] * df["w"]. When each row has to become a Python object, loop over itertuples() or to_dict("records").
samples/bad.py:6:5: error [pandas/chained-assignment] Chained assignment writes to a copy under copy-on-write, so the frame keeps its old values. Assign to df.loc[i, "a"] instead.
samples/bad.py:7:1: warning [pandas/no-inplace] inplace=True gives no speedup and blocks chaining. Reassign instead.
samples/bad.py:8:5: warning [pandas/merge-without-validate] merge without validate= can silently duplicate rows.
samples/bad.py:11:5: warning [pandas/concat-in-loop] out grows by one concat per pass, and each concat copies every earlier row. Collect the frames in a list and concat once after the loop.
xpd: 5 issues, 2 more with --fix --unsafe-fixes
```

## Install

```sh
uv tool install xpd   # or: uvx xpd check .
```

Before version 0.7.0 the package was `pdx-lint` and the command was `pdx`.

## Use

| Command | What it does |
| --- | --- |
| `xpd check [paths]` | Lint files and folders. Exit 1 on any error, 2 if xpd can't run. |
| `xpd check --fix` | Apply safe fixes, then lint. Add `--unsafe-fixes` to apply the rest. |
| `xpd profile data.csv` | Show dtypes, nulls, unique counts and memory per column. Needs pandas. |
| `xpd equiv old.py new.py` | Check that two scripts leave an equal `result`. Needs pandas. |
| `xpd graph features.py` | Print the dataflow of step code or Apache Hamilton modules: what reads what, and what does not connect. |
| `xpd hook` | Lint the file a Claude Code edit touched. |
| `xpd skill` | Print the agent skill for this version. |

Run `xpd -h` for every flag.

## Documentation

The full guide, including the Claude Code hook setup and how to write a rule, lives at
<https://bjoernaagaard.github.io/xpd/>.

## License

MIT or Apache-2.0, at your choice.
