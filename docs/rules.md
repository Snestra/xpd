# Rules

xpd has four rule packs: [pandas](#pandas), [scikit-learn](#scikit-learn), [Hamilton](#hamilton) and
[Python](#python). A library's rules run only on files that import it. The Python rules run on
every file.

## pandas

Every rule below has a direct heading link, its severity, fix safety, and before and after code.
Examples assume `import pandas as pd` and `import numpy as np`. Names such as `df`,
`output_columns` and `mapping` represent values supplied by your program. The replacement may
change behavior when repairing a bug or using an unsafe fix. Read the notes before applying it.

The rules follow pandas 3, where copy-on-write is always on. A fix that writes `pd.` or `np.`
applies only in a file with `import pandas as pd` or `import numpy as np`. In any other file xpd
reports the issue without a fix.

One rule asks for an API that pandas 3.1 added, `concat-then-category`. xpd reads the installed
pandas version from the name of the `pandas-<version>.dist-info` folder in the project's venv,
which is `$VIRTUAL_ENV` or the nearest `.venv` above the working directory, without starting
Python. On pandas 3.0 it skips that rule. When the venv has no pandas, or there is no venv, every
rule runs.

### Code that is wrong or broken on pandas 3

Each old form gives a wrong result, raises, or warns on pandas 3.0 and 3.1.

#### chained-assignment

**Severity:** error. **Fix:** unsafe: `df.loc[i, "a"] = v`. A write through `.iloc` or `.iat` on a column, and `update` on a column, have no fix.

Reports `df["a"][i] = v`, `df[mask]["a"] = v`, `df.loc[mask]["a"] = v`, `df.a[mask] = v`, `df["a"].iloc[0] = v` and `df["a"].update(other)`, which write to a copy.

Example:

```python
df["load"][row] = value
```

Use instead:

```python
df.loc[row, "load"] = value
```

#### inplace-on-column

**Severity:** error. **Fix:** unsafe: `df["b"] = df["b"].fillna(0)`.

Reports `df["b"].fillna(0, inplace=True)`, which changes a copy.

Example:

```python
df["load"].fillna(0, inplace=True)
```

Use instead:

```python
df["load"] = df["load"].fillna(0)
```

#### inplace-result-assigned

**Severity:** error. **Fix:** unsafe: `df = df.dropna()`.

Reports `df = df.dropna(inplace=True)`, which leaves `df` as None.

Example:

```python
df = df.dropna(inplace=True)
```

Use instead:

```python
df = df.dropna()
```

#### no-inplace

**Severity:** warning. **Fix:** unsafe: `df = df.dropna()`.

Reports any other `inplace=True`, which gives no speedup and blocks chaining.

Example:

```python
df.dropna(inplace=True)
```

Use instead:

```python
df = df.dropna()
```

#### epoch-from-int-cast

**Severity:** error. **Fix:** none.

Reports `df["d"].astype("int64") // 10**9`, `index.asi8 // 10**9` and `df["d"].values.astype("float64") / 1e9`, which are off by 1,000 now that datetimes default to microseconds.

Example:

```python
seconds = df["time"].astype("int64") // 10**9
```

Use instead:

```python
seconds = (df["time"] - pd.Timestamp("1970-01-01")) // pd.Timedelta("1s")
```

The epoch example assumes dates without a time zone. For dates with a time zone, use a UTC epoch. Missing timestamps remain missing.

The replacement is advice. xpd does not apply it automatically.

#### compare-to-missing

**Severity:** error. **Fix:** unsafe: `df["b"].isna()`.

Reports `df["b"] == np.nan`, `df["k"].astype(str) == "nan"` and `df[df["b"] == None]`, which are never true.

Example:

```python
missing = df["load"] == np.nan
```

Use instead:

```python
missing = df["load"].isna()
```

#### in-column

**Severity:** error. **Fix:** unsafe: `(df["a"] == 2).any()`.

Reports `2 in df["a"]`, which checks the index labels.

Example:

```python
found = 2 in df["load"]
```

Use instead:

```python
found = (df["load"] == 2).any()
```

#### mask-precedence

**Severity:** error. **Fix:** none.

Reports `df["a"] > 1 & df["b"] < 3`, where `&` binds first.

Example:

```python
selected = df[df["load"] > 1 & df["wind"] < 3]
```

Use instead:

```python
selected = df[(df["load"] > 1) & (df["wind"] < 3)]
```

The replacement is advice. xpd does not apply it automatically.

#### write-through-values

**Severity:** error. **Fix:** none.

Reports `df["a"].values[0] = 9`, which raises because the array is read-only.

Example:

```python
df["load"].values[0] = value
```

Use instead:

```python
df.loc[df.index[0], "load"] = value
```

The replacement is advice. xpd does not apply it automatically.

#### object-dtype-check

**Severity:** error. **Fix:** unsafe: `pd.api.types.is_string_dtype(df["k"])`. `is_object_dtype`, a test with `in`, `dtype.name` and `select_dtypes("object")` by position have no fix.

Reports `df["k"].dtype == object`, `select_dtypes(include="object")`, `is_object_dtype(df["k"])`, `df["k"].dtype in ("object", "category")` and `df["k"].dtype.name == "object"`, which miss the new str dtype.

Example:

```python
is_text = df["name"].dtype == object
```

Use instead:

```python
is_text = pd.api.types.is_string_dtype(df["name"])
```

#### groupby-apply-reads-key

**Severity:** error. **Fix:** none.

Reports `df.groupby("k").apply(lambda g: g["k"])`, which raises KeyError.

Example:

```python
keys = df.groupby("region").apply(lambda group: group["region"].iloc[0])
```

Use instead:

```python
keys = df.groupby("region").apply(lambda group: group.name)
```

Read group.name only inside the function passed to apply. Multiple group keys give a tuple.

The replacement is advice. xpd does not apply it automatically.

#### removed-in-pandas-3

**Severity:** error. **Fix:** safe: `map`, `ffill()`, `"ME"`, `"h"`, `"min"`, `sep=r"\s+"`, `lineterminator=`, `sep=` for a separator passed by position, `is_monotonic_increasing`, `np.` for `pd.np.` and `pd.json_normalize`. unsafe: `"M"` in `resample`, `dt.isocalendar().week`, `writer.close()` and `pd.Series(x).value_counts()`. The other forms have no fix.

Reports `applymap`, `fillna(method=)`, frequencies such as `"M"`, `"H"` and `"T"`, `delim_whitespace=True`, `dt.week`, and the removed arguments and names listed below.

Example:

```python
filled = df["load"].fillna(method="ffill")
months = df.resample("M", on="time")["load"].sum()
```

Use instead:

```python
filled = df["load"].ffill()
months = df.resample("ME", on="time")["load"].sum()
```

#### reduction-level-argument

**Severity:** error. **Fix:** unsafe: `df.groupby(level=0).sum()`.

Reports `df.sum(level=0)` and the other reductions with `level=`, which raise TypeError.

Example:

```python
totals = df["load"].sum(level="region")
```

Use instead:

```python
totals = df["load"].groupby(level="region").sum()
```

#### stack-dropna

**Severity:** error, or warning for `future_stack=False`. **Fix:** none.

Reports `df.stack(dropna=False)`, which raises ValueError, and `df.stack(future_stack=False)`, which is deprecated.

Example:

```python
long = wide.stack(dropna=True)
```

Use instead:

```python
long = wide.stack().dropna(how="all")
```

For dropna=False, write stack() alone. The new stack keeps the rows that are all missing, which the old default dropped.

The replacement is advice. xpd does not apply it automatically.

#### frame-append

**Severity:** error. **Fix:** none.

Reports `df = df.append(row)` and `df.append(row, ignore_index=True)`, which raise AttributeError.

Example:

```python
out = pd.DataFrame()
for part in parts:
    out = out.append(part)
```

Use instead:

```python
out = pd.concat(parts)
```

The example assumes parts is not empty. For single rows, collect them in a list and build the frame once.

The replacement is advice. xpd does not apply it automatically.

#### str-replace-regex-default

**Severity:** error. **Fix:** unsafe: `regex=True`.

Reports `df["s"].str.replace(r"\d+", "")`, which replaces nothing, because `str.replace` reads its pattern as plain text since pandas 2.0.

Example:

```python
digits = df["phone"].str.replace(r"\D", "")
```

Use instead:

```python
digits = df["phone"].str.replace(r"\D", "", regex=True)
```

#### positional-key-on-labels

**Severity:** error. **Fix:** unsafe: `.iloc[0]`.

Reports `df["k"].value_counts()[0]` and `df.iloc[0][0]`, which raise KeyError, because an integer key on a Series is a label on pandas 3.

Example:

```python
top = df["region"].value_counts()[0]
first = df.iloc[0][0]
```

Use instead:

```python
top = df["region"].value_counts().iloc[0]
first = df.iloc[0, 0]
```

#### deprecated-in-pandas-3

**Severity:** warning. **Fix:** safe: delete the line, drop `copy=`, `pd.Timestamp.now("UTC")`. unsafe: drop `copy=` from `astype`.

Reports `pd.set_option("mode.copy_on_write", True)`, `copy=False` and `pd.Timestamp.utcnow()`.

Example:

```python
now = pd.Timestamp.utcnow()
```

Use instead:

```python
now = pd.Timestamp.now("UTC")
```


`removed-in-pandas-3` also reports these, each of which raises on pandas 3.1:

- Arguments of `read_csv`: `error_bad_lines`, `warn_bad_lines`, `squeeze`, `verbose`, `keep_date_col`,
  `infer_datetime_format`, `mangle_dupe_cols`, `prefix`, `date_parser`, and a separator passed by position.
  For bad lines, pass `on_bad_lines="skip"`.
- Writing: `to_csv(line_terminator=)`, a separator passed to `to_csv` by position, `to_excel(encoding=)`,
  `read_excel(squeeze=)` and `writer.save()` on a name that holds the word writer.
- Names: `pd.value_counts`, `is_monotonic`, `mad`, `as_matrix`, `ix`, `pd.np`, `pd.datetime`,
  `pd.Int64Index`, `pd.UInt64Index`, `pd.Float64Index`, `pd.io.json.json_normalize`, `pandas.util.testing`,
  `SettingWithCopyWarning`, the option `mode.use_inf_as_na` and `pd.to_numeric(errors="ignore")`.
- Time series: `tshift`, `pad()`, `first("3D")` and `last("3D")`, `resample(loffset=, base=, kind=)`,
  `date_range(closed=)`, `to_datetime(infer_datetime_format=)` and `to_datetime(errors="ignore")`,
  `pct_change(fill_method="pad")`, `pd.Timedelta("5T")`, and `astype("timedelta64[D]")`.
- `iteritems()`, `is_lexsorted()`, `lexsort_depth` and `groupby(axis=1)`.

The `writer.close()` fix is unsafe, because xpd knows the writer only by its name. `DataFrame.lookup`
was removed too and is not reported, because other libraries have a method of that name.

The new `chained-assignment` shapes: `df.a[mask] = v` is reported when the mask reads the same
frame, which tells it from `self.items[i > 3] = v`. `df["a"].update(other)` is reported only on a name such as `df` or
`frame`, because `state["seen"].update(extra)` is how a dict of dicts is updated. `compare-to-missing` reports `None` only
inside a subscript, because `config["mode"] == None` on a dict is valid Python. `object-dtype-check`
leaves `dtype.kind == "O"` alone, which is still true for a text column.

`str-replace-regex-default` reports only a pattern that cannot be meant as plain text: a class such
as `\d`, a set in brackets, `.*` or `.+`, a `^` at the start, or `a|b`. It leaves `"."`, `"$"` and
`"("` alone. Its fix is unsafe, because a set in brackets can be meant as text.
`positional-key-on-labels` reports `value_counts()[0]`, `df.iloc[i][0]`, `df.dtypes[0]` and a frame
reduction such as `df.isna().sum()[0]`. Its fix is unsafe: on labels that are whole numbers the old
form still works, and there `value_counts()[0]` is the count of the value 0. The frame reductions have no fix.

The `reduction-level-argument` fix applies when `level=` is the only argument, and it is unsafe
because the old form does not run on pandas 3, so the two cannot be compared. `stack-dropna` has no
fix, because the replacement depends on the value of `dropna`. `frame-append` reports an `append`
whose result is assigned, returned, passed on or read, or that passes `ignore_index`,
`verify_integrity` or `sort`, because `list.append` returns None and takes no keywords. It leaves
`df.index.append(x)` and `df.columns.append(x)` alone, because `Index.append` still exists. An
Index held in a plain name, as in `labels = labels.append(more)`, is reported by mistake.

The `chained-assignment` fix covers the three shapes above. A mask held in a name is fixed when
the enclosing function assigns that name only masks, such as a comparison or a call to `isin`.
Any other chain, such as `df.iloc[0]["a"] = v`, is reported without a fix. Two plain keys, as in
`rows[0]["name"] = v`, are left alone, because they are more often a list or a dict.

`in-column` and `inplace-on-column` can report a dict, such as `"x" in data["names"]`, because xpd
cannot tell it from a frame. Their fixes are unsafe for that reason.

`removed-in-pandas-3` rewrites a frequency where the call takes only datetimes, such as
`date_range`, `Grouper`, `floor` and `rolling`. Inside `resample` and `shift` the fix for `"M"`,
`"Q"` and `"Y"` is unsafe, because those aliases stay valid when the index holds periods. It
leaves `to_period("M")` and `asfreq("M")` alone, and it reports `"A"` inside `resample`, `asfreq`
and `shift` without a fix, because the new alias is `"YE"` for datetimes and `"Y"` for periods.

`deprecated-in-pandas-3` deletes the copy-on-write option only at the top level of a file, and
reports it without a fix inside a function or another block. Dropping `copy=` from `astype` is
unsafe, because a numpy array also has `astype(copy=False)`, and there it can return the same
array.

The `no-inplace` fix applies only to a call that stands alone as a statement, on a name, attribute
or subscript. The inplace fixes drop comments between the call's arguments.

### Shorter forms, including the pandas 3.0 and 3.1 API

These rules are warnings. Most of them make code shorter and do not change its speed.

#### sort-then-reset-index

**Severity:** warning. **Fix:** safe: `df.sort_values("b", ignore_index=True)`.

Reports `df.sort_values("b").reset_index(drop=True)`, also after `sort_index`, `dropna`, `drop_duplicates`, `explode` and `sample`.

Example:

```python
ordered = df.sort_values("load").reset_index(drop=True)
```

Use instead:

```python
ordered = df.sort_values("load", ignore_index=True)
```

#### set-index-then-resample

**Severity:** warning. **Fix:** safe: `df.resample("D", on="d").sum()`.

Reports `df.set_index("d").resample("D").sum()`.

Example:

```python
daily = df.set_index("time").resample("D")["load"].sum()
```

Use instead:

```python
daily = df.resample("D", on="time")["load"].sum()
```

#### subtract-shift

**Severity:** warning. **Fix:** unsafe: `df["b"].diff()`.

Reports `df["b"] - df["b"].shift()`.

Example:

```python
change = df["load"] - df["load"].shift()
```

Use instead:

```python
change = df["load"].diff()
```

#### range-mask

**Severity:** warning. **Fix:** unsafe: `df["a"].between(10, 20)`.

Reports `(df["a"] >= 10) & (df["a"] <= 20)`.

Example:

```python
inside = (df["load"] >= 10) & (df["load"] <= 20)
```

Use instead:

```python
inside = df["load"].between(10, 20)
```

`between` is a method of a Series. An index and a NumPy array have none, so comparisons on
`df.index`, `df.index.hour`, `df.columns`, `df["load"].values` and `df["load"].array` are left
alone.

#### groupby-then-reset-index

**Severity:** warning. **Fix:** unsafe: `df.groupby("k", as_index=False)[["a", "b"]].sum()`.

Reports `df.groupby("k")[["a", "b"]].sum().reset_index()`.

Example:

```python
totals = df.groupby("region")["load"].sum().reset_index()
```

Use instead:

```python
totals = df.groupby("region", as_index=False)["load"].sum()
```

#### rename-after-agg

**Severity:** warning. **Fix:** none.

Reports `g.agg({"b": "mean"}).rename(columns={"b": "b_mean"})`.

Example:

```python
summary = df.groupby("region").agg({"load": "mean"}).rename(columns={"load": "mean_load"})
```

Use instead:

```python
summary = df.groupby("region").agg(mean_load=("load", "mean"))
```

The replacement is advice. xpd does not apply it automatically.

#### merge-indicator-anti-join

**Severity:** warning. **Fix:** none.

Reports `m[m["_merge"] == "left_only"]` after `merge(..., indicator=True)`.

Example:

```python
merged = left.merge(right, on="key", how="left", indicator=True)
unmatched = merged[merged["_merge"] == "left_only"]
```

Use instead:

```python
unmatched = left.merge(right, on="key", how="left_anti")
```

The anti join returns the left columns, without the indicator or the right-hand columns. Use it when the result needs unmatched left rows rather than the full merged schema.

The replacement is advice. xpd does not apply it automatically.

#### value-counts-share

**Severity:** warning. **Fix:** unsafe: `df["k"].value_counts(normalize=True)`.

Reports `df["k"].value_counts() / len(df["k"])`.

Example:

```python
shares = df["region"].value_counts() / len(df["region"])
```

Use instead:

```python
shares = df["region"].value_counts(normalize=True)
```

#### nested-np-where

**Severity:** warning. **Fix:** none.

Reports `np.where(a < 20, "low", np.where(a < 70, "mid", "high"))`.

Example:

```python
band = np.where(df["load"] < 20, "low", np.where(df["load"] < 70, "mid", "high"))
```

Use instead:

```python
band = pd.Series("high", index=df.index).case_when([
    (df["load"] < 20, "low"),
    (df["load"] < 70, "mid"),
])
```

case_when uses the first matching condition. It returns a Series rather than a NumPy array.

The replacement is advice. xpd does not apply it automatically.

#### query-string

**Severity:** warning. **Fix:** none.

Reports `df.query("a > 50 and b < 0")`.

Example:

```python
selected = df.query("load > 50 and wind < 3")
```

Use instead:

```python
selected = df.loc[(pd.col("load") > 50) & (pd.col("wind") < 3)]
```

The replacement is advice. xpd does not apply it automatically.

#### filter-items

**Severity:** warning. **Fix:** unsafe: `df[cols]`.

Reports `df.filter(items=cols)`.

Example:

```python
selected = df.filter(items=output_columns)
```

Use instead:

```python
selected = df[output_columns]
```

The replacement raises when a requested column is absent. filter silently skips it, which is why the fix is unsafe.

#### inline-column-list

**Severity:** warning. **Fix:** none.

Reports `df[["k", "a", "b"]]` and `df.groupby(["k", "s", "raw"])`, three or more names written inside the expression.

Example:

```python
selected = df[["load", "wind", "solar"]]
```

Use instead:

```python
output_columns = ["load", "wind", "solar"]
selected = df[output_columns]
```

The replacement is advice. xpd does not apply it automatically.

#### concat-then-category

**Severity:** warning. **Fix:** unsafe: `pd.concat(parts, union_categories=True)`.

Reports `pd.concat(parts).astype("category")`, on pandas 3.1.

Example:

```python
combined = pd.concat(parts).astype("category")
```

Use instead:

```python
combined = pd.concat(parts, union_categories=True)
```


xpd has no rule for or against a lambda in `assign`. A lambda and `pd.col` gave the same result
at the same speed, and a lambda also works with `np.where` and with calculations across columns.
`set-index-then-resample` applies only when a named aggregation such as `sum` or `mean` follows,
because upsampling with `asfreq` raises with `on=`, and a function passed to `agg` sees other
data. The `range-mask` fix is unsafe, because the operand may be a numpy array, which has no
`between`. The `subtract-shift` fix is unsafe, because `diff` wraps around on an unsigned integer
column and returns booleans for a boolean column. The `groupby-then-reset-index` fix is unsafe,
because a key that is not a column comes back in another place. The `value-counts-share` fix names
the result proportion instead of count and leaves missing values out of the total.
`query-string` leaves a string that reads like SQL alone, such as `conn.query("SELECT ...")`, and
it asks for `df.loc[pd.col("a") > 50]`, which type checkers accept. The `filter-items` fix is
unsafe, because `df[cols]` raises on a label that is absent, where `filter` skipped it.
`inline-column-list` reports a list of three or more text literals in `df[[...]]`, in
`df.loc[rows, [...]]`, as an argument of `groupby`, `sort_values`, `set_index`, `select` or
`drop_duplicates`, and in `by=`, `subset=`, `on=`, `left_on=`, `right_on=`, `columns=`,
`id_vars=`, `value_vars=` or `usecols=`. It leaves `pd.DataFrame(data, columns=[...])` alone,
because the list there defines the frame. It asks for a named list, such as
`output_columns = ["k", "a", "b"]`, passed by name. The `concat-then-category`
fix is unsafe, because the category order differs, and parts that are not categorical stay so.

### Python work per row or per value

#### apply-known-function

**Severity:** warning. **Fix:** safe: `df["s"].str.len()`. unsafe: `df["b"].isna()`, `pd.to_datetime(df["raw"])`, `df["b"].round(2)`.

Reports `df["s"].apply(len)`, `df["b"].apply(pd.isna)`, `df["raw"].apply(pd.to_datetime)` and `df["b"].apply(lambda v: round(v, 2))`.

Example:

```python
lengths = df["name"].apply(len)
```

Use instead:

```python
lengths = df["name"].str.len()
```

#### literal-contains

**Severity:** warning. **Fix:** safe: `str.contains("ta_", regex=False)`.

Reports `df["s"].str.contains("ta_")`, a pattern with no regex character.

Example:

```python
matches = df["name"].str.contains("load_")
```

Use instead:

```python
matches = df["name"].str.contains("load_", regex=False)
```

#### copy-after-indexing

**Severity:** warning. **Fix:** safe: drop `.copy()`.

Reports `df[["a", "b"]].copy()`, `df[mask].copy()` and `df.loc[...].copy()`.

Example:

```python
part = df[["load", "wind"]].copy()
```

Use instead:

```python
part = df[["load", "wind"]]
```

#### lookup-per-row

**Severity:** warning. **Fix:** unsafe: `df["k"].map(table["v"])`. A lookup through a mask has no fix.

Reports `df["k"].apply(lambda key: table.loc[key, "v"])` and `lambda key: table[table["k"] == key]["v"].iloc[0]`, which read another frame once per row.

Example:

```python
df["price"] = df["item"].apply(lambda item: prices.loc[item, "price"])
```

Use instead:

```python
df["price"] = df["item"].map(prices["price"])
```

#### apply-recomputes-aggregate

**Severity:** warning. **Fix:** none.

Reports `df["b"].apply(lambda v: v - df["b"].mean())`, which computes the mean of the whole column again for every row.

Example:

```python
centered = df["load"].apply(lambda value: value - df["load"].mean())
```

Use instead:

```python
centered = df["load"] - df["load"].mean()
```

The replacement is advice. xpd does not apply it automatically.

#### enumerate-map-encoding

**Severity:** warning. **Fix:** none.

Reports `df["k"].map({v: i for i, v in enumerate(df["k"].unique())})`, a dict of codes built by hand.

Example:

```python
codes = df["region"].map({name: code for code, name in enumerate(df["region"].unique())})
```

Use instead:

```python
codes = pd.factorize(df["region"])[0]
```

factorize numbers the values in the order they first appear, as the dict does. It returns a NumPy array, and it gives a missing value the code -1 where the dict gave it a code of its own. Pass `sort=True` for the order of `sorted`.

The replacement is advice. xpd does not apply it automatically.

#### copy-in-chain

**Severity:** warning. **Fix:** safe: drop `.copy()`.

Reports `df.copy().rename(...)` and `df.rename(...).copy()`, also with `assign`, `reset_index`, `set_index`, `astype`, `fillna`, `drop(columns=)` and `pd.concat`.

Example:

```python
clean = df.copy().rename(columns=names)
tidy = df.reset_index(drop=True).copy()
```

Use instead:

```python
clean = df.rename(columns=names)
tidy = df.reset_index(drop=True)
```

#### row-loop

**Severity:** warning. **Fix:** none.

Reports a loop over `iterrows()`, a loop over `itertuples()` or `to_dict("records")` that only computes from the row values, and `for i in range(len(df))` with `df.iloc[i]`.

Example:

```python
total = sum(row.load * row.weight for row in df.itertuples())
```

Use instead:

```python
total = (df["load"] * df["weight"]).sum()
```

The example sums numeric columns without missing values. Check missing values and floating point rounding when replacing a Python reduction. Keep a loop when its work cannot be expressed on columns.

A loop over `itertuples()` or `to_dict("records")` that hands the row or its values to a function,
as in `orders = [Order(id=row["id"], total=row["total"]) for row in df.to_dict("records")]`, turns
rows into Python objects. Columns cannot do that, so it is left alone. A cast such as
`int(row["a"])` and a list method such as `out.append(...)` do not count as a function here.
`iterrows()` is reported in every loop, because it builds a Series for each row: use
`itertuples()` or `to_dict("records")` when each row has to become an object.

The replacement is advice. xpd does not apply it automatically.

#### apply-axis-1

**Severity:** warning. **Fix:** none.

Reports `df.apply(f, axis=1)`, and `agg` and `transform` with `axis=1` and a lambda or a `"_".join`.

Example:

```python
total = df.apply(lambda row: row["load"] + row["wind"], axis=1)
```

Use instead:

```python
total = df["load"] + df["wind"]
```

The replacement is advice. xpd does not apply it automatically.

#### apply-text-method

**Severity:** warning. **Fix:** unsafe: `df["s"].str.startswith("A")`.

Reports `df["s"].map(lambda v: v.startswith("A"))`.

Example:

```python
names = df["name"].map(lambda name: name.upper())
```

Use instead:

```python
names = df["name"].str.upper()
```

#### apply-date-part

**Severity:** warning. **Fix:** unsafe: `df["d"].dt.year`.

Reports `df["d"].apply(lambda t: t.year)`.

Example:

```python
years = df["time"].apply(lambda time: time.year)
```

Use instead:

```python
years = df["time"].dt.year
```

#### apply-condition

**Severity:** warning. **Fix:** none.

Reports `df["a"].apply(lambda v: 1 if v > 50 else 0)` and `lambda v: min(v, 1.0)`.

Example:

```python
flag = df["load"].apply(lambda value: 1 if value > 50 else 0)
```

Use instead:

```python
flag = pd.Series(np.where(df["load"] > 50, 1, 0), index=df.index)
```

The explicit Series constructor preserves the row labels. Check the result name, dtype and missing-value behavior.

The replacement is advice. xpd does not apply it automatically.

#### apply-arithmetic

**Severity:** warning. **Fix:** unsafe: `df["b"] * 2 + 1`.

Reports `df["b"].apply(lambda v: v * 2 + 1)`.

Example:

```python
scaled = df["load"].apply(lambda value: value * 2 + 1)
```

Use instead:

```python
scaled = df["load"] * 2 + 1
```

#### builtin-on-column

**Severity:** warning. **Fix:** unsafe: `df["a"].sum()`, `df["k"].nunique()`, `(df["a"] > 50).sum()`, `set(df["k"].unique())`, `df["a"].tolist()`, `df["a"].to_numpy()`. `Counter` has no fix.

Reports `sum(df["a"])`, `len(set(df["k"]))`, `sum(df["a"] > 50)`, `set(df["k"])`, `Counter(df["k"])`, `list(df["a"])` and `np.array(df["a"].tolist())`.

Example:

```python
total = sum(df["load"])
```

Use instead:

```python
total = df["load"].sum()
```

#### list-over-column

**Severity:** warning. **Fix:** none.

Reports `[v.upper() for v in df["s"]]`, and a `for` loop over a column that appends to a list.

Example:

```python
names = [name.upper() for name in df["name"]]
```

Use instead:

```python
names = df["name"].str.upper()
```

The result is a labeled Series rather than a Python list. Use tolist only at a boundary that needs a list.

The replacement is advice. xpd does not apply it automatically.

#### frame-growth-in-loop

**Severity:** warning. **Fix:** none.

Reports `out.loc[len(out)] = row` inside a loop.

Example:

```python
out = pd.DataFrame(columns=output_columns)
for row in rows:
    out.loc[len(out)] = row
```

Use instead:

```python
out = pd.DataFrame(rows, columns=output_columns)
```

The replacement is advice. xpd does not apply it automatically.

#### len-of-filter

**Severity:** warning. **Fix:** unsafe: `int((df["a"] > 50).sum())`.

Reports `len(df[df["a"] > 50])`.

Example:

```python
count = len(df[df["load"] > 50])
```

Use instead:

```python
count = int((df["load"] > 50).sum())
```

#### sort-then-head

**Severity:** warning. **Fix:** unsafe: `df.nsmallest(10, "b")`, `df.iloc[df["b"].argmin()]`, `df["b"].min()`, `df["b"].max()`, `df["b"].idxmin()`. The last rows of a frame, `iloc[-1]` on a sorted column and `sorted(df["b"])[0]` have no fix.

Reports `df.sort_values("b").head(10)`, `df.sort_values("b").iloc[0]`, `df["b"].sort_values().iloc[0]`, `df.sort_values("b").tail(10)` and `sorted(df["b"])[0]`.

Example:

```python
largest = df.sort_values("load", ascending=False).head(10)
```

Use instead:

```python
largest = df.nlargest(10, "load")
```

#### replace-with-mapping

**Severity:** warning. **Fix:** none.

Reports `df["k"].replace(mapping)` with a mapping held in a name.

Example:

```python
names = df["name"].replace(mapping)
```

Use instead:

```python
names = df["name"].map(lambda value: mapping.get(value, value))
```

The example uses an ordinary dictionary and scalar values. A short mapping can be faster with replace. map does not reproduce regex replacement or every special mapping behavior.

The replacement is advice. xpd does not apply it automatically.

#### concat-in-loop

**Severity:** warning. **Fix:** none.

Reports a frame that grows inside a loop, as in `out = pd.concat([out, part])`. Each pass copies every earlier row.

Example:

```python
out = pd.DataFrame()
for path in paths:
    out = pd.concat([out, pd.read_csv(path)])
```

Use instead:

```python
frames = [pd.read_csv(path) for path in paths]
out = pd.concat(frames)
```

The example assumes paths is not empty and the starting frame has no schema to preserve. A single concat avoids repeatedly copying the growing result.

A `pd.concat` inside a loop that builds a frame from other objects, such as
`panel = pd.concat(parts_of_this_month)`, copies each row once and is left alone.

The replacement is advice. xpd does not apply it automatically.

#### apply-math-function

**Severity:** warning. **Fix:** unsafe: `np.sqrt(df["w"])`, `df["b"].abs()`.

Reports `df["w"].map(np.sqrt)`, `df["w"].apply(lambda v: np.log(v))`, `df["w"].apply(math.sqrt)` and `df["b"].apply(abs)`.

Example:

```python
root = df["wind"].map(np.sqrt)
```

Use instead:

```python
root = np.sqrt(df["wind"])
```

#### apply-cast

**Severity:** warning. **Fix:** unsafe: `df["b"].astype(int)`.

Reports `df["b"].apply(int)`, `df["raw"].map(float)` and `df["b"].apply(lambda v: int(v))`.

Example:

```python
whole = df["load"].apply(int)
```

Use instead:

```python
whole = df["load"].astype(int)
```

#### apply-comparison

**Severity:** warning. **Fix:** unsafe: `df["a"] > 50`, `df["a"].isin(wanted)`.

Reports `df["a"].apply(lambda v: v > 50)` and `df["a"].apply(lambda v: v in wanted)`.

Example:

```python
high = df["load"].apply(lambda value: value > 50)
```

Use instead:

```python
high = df["load"] > 50
```

#### apply-pd-series

**Severity:** warning. **Fix:** unsafe: `pd.DataFrame(df["pair"].tolist(), index=df["pair"].index)`.

Reports `df["pair"].apply(pd.Series)`, which builds one Series per row to split a column of lists or dicts.

Example:

```python
parts = df["pair"].apply(pd.Series)
```

Use instead:

```python
parts = pd.DataFrame(df["pair"].tolist(), index=df["pair"].index)
```

#### np-vectorize

**Severity:** warning. **Fix:** none.

Reports `np.vectorize(f)`, `np.frompyfunc(f, 2, 1)` and `np.apply_along_axis(f, 1, values)`, which still call Python once per value or per row.

Example:

```python
def bigger(load, wind):
    if load > wind:
        return load * 2
    return wind

out = np.vectorize(bigger)(df["load"], df["wind"])
```

Use instead:

```python
out = np.where(df["load"] > df["wind"], df["load"] * 2, df["wind"])
```

The replacement is advice. xpd does not apply it automatically.

#### index-loop

**Severity:** warning. **Fix:** none.

Reports `for label in df.index` with `df.loc[label, "a"]` or `df.at[label, "a"]` in the body, also as a comprehension.

Example:

```python
out = []
for label in df.index:
    out.append(df.loc[label, "load"] * df.loc[label, "weight"])
```

Use instead:

```python
out = (df["load"] * df["weight"]).tolist()
```

The replacement is advice. xpd does not apply it automatically.

#### zip-over-columns

**Severity:** warning. **Fix:** none.

Reports a loop or comprehension over `zip(df["a"], df["w"])`.

Example:

```python
out = [load * weight for load, weight in zip(df["load"], df["weight"])]
```

Use instead:

```python
out = (df["load"] * df["weight"]).tolist()
```

Keep the result as a Series when the next step takes one. Use tolist only at a boundary that needs a list.

The replacement is advice. xpd does not apply it automatically.

On 1,000,000 rows `map` with a Series ran 50 to 64 times faster than the `.loc` lambda of
`lookup-per-row`, measured on 100,000 rows, and 777 to 848 times faster than the mask form, measured
on 20,000. Its fix is unsafe, because a key that is absent gives a missing value where the lambda
raised. `apply-recomputes-aggregate` grows with the square of the row count: 20,000 rows took 0.34
seconds against 0.2 milliseconds. It reports a reduction of a column taken by a text label, and
leaves `lambda v: v["scores"].mean()` alone, which reads the row's own value.
`enumerate-map-encoding` was 7 to 17 times slower than `pd.factorize`. `copy-in-chain` names only
methods that return a new frame at once: each copy cost 4 to 10 milliseconds on 6 columns, and
dropping it was 3 to 118 times faster. A copy next to a slow method such as `sort_values` or `merge`
is as wasted and is not reported, because it gained under 2 times. `df.copy().pipe(f)` is left
alone, because `f` may write to its argument.

The `apply-axis-1` shapes with `agg` and `transform` were 327 to 2,735 times slower than whole
columns on 100,000 rows. A named method, as in `agg("sum", axis=1)`, is left alone. The new
`builtin-on-column` shapes were 2.6 to 483 times slower, and `list` and `set` can report a dict of
lists, such as `list(config["names"])`, because xpd cannot tell it from a frame. The `for` loop of
`list-over-column` was 13 to 19 times slower. `sort-then-head` reports `sort_values().iloc[-1]`
without a fix, because the sort puts missing values last, so the old form returns NaN where `max()`
skips them. Its `idxmin` fix can name another row when the lowest value appears twice.

`apply-known-function` reports only a column taken by a text label, because on a frame `apply`
works per column. Its `pd.to_datetime` fix is unsafe, because a call per value parses each text
on its own, and the whole column must share one format. The `isna` fix is unsafe, because on a
column of lists `pd.isna` per value returned an array per cell. The `round` fix is unsafe, because
Python and numpy round some halfway values differently. `copy-after-indexing` leaves a single text
label and a slice alone, because on a dict or a numpy array the copy keeps a later write away
from the original.

`apply-math-function` leaves `df["w"].apply(np.sqrt)` alone, because `Series.apply` already hands
a NumPy function the whole column, and `map` does not. Its fix is unsafe, because `math.sqrt` and
`math.log` raise on a value outside their domain, where NumPy returns NaN. `apply-cast` covers
`int` and `float` only: on floats with missing values `astype(str)` was slower than `apply(str)`
and kept the missing values. `apply-comparison` leaves a chain such as `0 < v < 5` alone, and a
test on text such as `"ta" in v` or `v in "abc"`. Its `isin` fix is unsafe, because `in` asks the
container, and a dict, a range or a custom container can answer differently. The
`apply-pd-series` fix is unsafe, because the old form turned whole numbers into floats when a row
also held floats, and the fix keeps them whole. With a bare function name, `apply-math-function`,
`apply-cast` and `apply-pd-series` report `apply` only on a column taken by a text label, because
on a frame `apply` works per column. `index-loop` leaves a loop that never reads the frame by its
label alone. `zip-over-columns` reports only when every argument of `zip` is a column taken by a
text label, or a `.values`, `.to_numpy()` or `.tolist()`. It leaves `dict(zip(...))` alone, which
is not a loop.

`row-loop` replaces the `no-iterrows` rule of xpd 0.1.0. The fixes of `apply-text-method`,
`apply-date-part` and `apply-arithmetic` are unsafe, because a column method returns a missing
value where the lambda raised, and the dtype can change: `dt.year` is int32 where the lambda gave
int64. `builtin-on-column` skips missing values after the fix. `sort-then-head` is unsafe, because
`nsmallest` raises on a text column and orders ties differently, and `argmin` raises when every
value is missing. `replace-with-mapping` asks for
`df["k"].map(lambda v: mapping.get(v, v))`, which keeps the values and the dtype.

### groupby, agg and transform

#### groupby-lambda

**Severity:** warning. **Fix:** safe: `"max"` for min, max, median, count and nunique. unsafe: `"sum"`, `"mean"`. A reduction with arguments, a count of a condition and `resample` have no fix.

Reports `g["b"].agg(lambda v: v.max())`, `g.agg(np.sum)`, `pivot_table(aggfunc=np.mean)`, `g.agg(lambda v: v.quantile(0.9))`, `g.agg(lambda v: (v > 0).sum())` and a lambda or NumPy function on `resample`.

Example:

```python
maximum = df.groupby("region")["load"].agg(lambda group: group.max())
```

Use instead:

```python
maximum = df.groupby("region")["load"].agg("max")
```

#### groupby-lambda-window

**Severity:** warning. **Fix:** safe: `g["b"].shift(1)`. unsafe: `g["b"].cumsum()`.

Reports `g["b"].transform(lambda v: v.shift(1))`.

Example:

```python
previous = df.groupby("region")["load"].transform(lambda group: group.shift(1))
```

Use instead:

```python
previous = df.groupby("region")["load"].shift(1)
```


`groupby-lambda` leaves a lambda inside a list alone, because the result column would get a new
name. For `g.agg(lambda v: v.quantile(0.9))` write `g.quantile(0.9)`, which was 4.8 times faster
with 1,000 groups and 345 times with 100,000. For a count of a condition write
`(df["b"] > 0).groupby(df["k"]).sum()`. On a resampler the named method was 3.4 to 82 times faster.
The `groupby-lambda-arithmetic` shapes `v - v.mean()` and `v / v.max()` were 6 times slower than
`transform` with a name with 1,000 groups and 390 times slower with 100,000. Min-max scaling has no
fix. It also leaves `np.median`, `np.std` and `np.var` alone, because they treat missing values
and degrees of freedom differently from the named methods. The fixes for sum, mean, std, var, prod,
cumsum and cumprod are unsafe, because pandas adds up each group with compensated summation. On
values such as 1e16, 1, 1 and -1e16 the sum changes from 0 to 2. `groupby-lambda-window` leaves
`shift(freq=)` alone, because it moves the index.

#### groupby-lambda-arithmetic

**Severity:** warning. **Fix:** unsafe: `df["w"] / df.groupby("k")["w"].transform("sum")`.

Reports `g["w"].transform(lambda v: v / v.sum())`, `v - v.mean()`, `v / v.max()`, the z-score, min-max scaling and `v.fillna(v.mean())`.

Example:

```python
share = df.groupby("region")["load"].transform(lambda group: group / group.sum())
```

Use instead:

```python
share = df["load"] / df.groupby("region")["load"].transform("sum")
```

#### groupby-filter-lambda

**Severity:** warning. **Fix:** unsafe: `df[df.groupby("k")["k"].transform("size") > 10]`.

Reports `df.groupby("k").filter(lambda g: len(g) > 10)`.

Example:

```python
selected = df.groupby("region").filter(lambda group: len(group) > 10)
```

Use instead:

```python
selected = df[df.groupby("region")["region"].transform("size") > 10]
```

The example assumes the group keys are not missing. Check dropna and missing-key behavior before rewriting a filter.

#### groupby-apply

**Severity:** warning. **Fix:** none.

Reports any `groupby(...).apply` with a lambda.

Example:

```python
totals = df.groupby("region")["load"].apply(lambda group: group.sum())
```

Use instead:

```python
totals = df.groupby("region")["load"].sum()
```

The built-in reduction can change the index name, the dtype and floating point rounding. Check the result before replacing a general apply.

The replacement is advice. xpd does not apply it automatically.

#### aggregate-then-merge-back

**Severity:** warning. **Fix:** none.

Reports `df.merge(df.groupby("k")["b"].sum()...)` and `df["k"].map(df.groupby("k")["b"].sum())`.

Example:

```python
result = df.merge(
    df.groupby("region")["load"].sum().rename("total").reset_index(),
    on="region",
    how="left",
    validate="many_to_one",
)
```

Use instead:

```python
result = df.assign(total=lambda frame: frame.groupby("region")["load"].transform("sum"))
```

transform keeps the original index and row order. A merge can instead create a fresh index. The report covers an inline aggregation on the same frame, as well as mapping an aggregate back to its source column. An aggregate stored in a separate variable is not detected.

The replacement is advice. xpd does not apply it automatically.

#### groupby-first-skips-missing

**Severity:** warning. **Fix:** none.

Reports `df.groupby("k").first()` and `last()`.

Example:

```python
first = df.groupby("region").first()
```

Use instead:

```python
first = df.groupby("region").first(skipna=False)
```

skipna=False preserves missing values from the first row. Without it, first can combine values from different rows.

The replacement is advice. xpd does not apply it automatically.


### MultiIndex

#### multiindex-from-tuples

**Severity:** warning. **Fix:** unsafe: `pd.MultiIndex.from_arrays([df["k"], df["a"]])`, `pd.MultiIndex.from_product([keys, numbers])`. The comprehension has no fix.

Reports `pd.MultiIndex.from_tuples(zip(df["k"], df["a"]))`, `from_tuples(itertools.product(keys, numbers))` and `from_tuples([(k, n) for k in keys for n in numbers])`, which build one Python tuple per row.

Example:

```python
index = pd.MultiIndex.from_tuples(list(zip(df["region"], df["year"])), names=["region", "year"])
```

Use instead:

```python
index = pd.MultiIndex.from_arrays([df["region"], df["year"]], names=["region", "year"])
```

#### level-values-unique

**Severity:** warning. **Fix:** unsafe: `df.index.unique("k")`, `df.index.unique("k").nunique()`.

Reports `df.index.get_level_values("k").unique()` and `.nunique()`, which build one value per row first.

Example:

```python
regions = df.index.get_level_values("region").unique()
```

Use instead:

```python
regions = df.index.unique("region")
```


The `multiindex-from-tuples` fix is unsafe, because without `names=` `from_arrays` takes the level
names from the Series it is given, where `from_tuples` left them empty. It leaves
`itertools.product(keys, repeat=2)` alone. The `level-values-unique` fix is unsafe, because a level
built from a range comes back as a RangeIndex, with the same values, where the old form gave an
Index. On 1,000,000 rows `from_arrays` ran 27 times faster than `from_tuples` over `zip`,
`from_product` 118 times faster than the comprehension, and `unique` with a level 22 times faster.

### Time series

#### rolling-apply

**Severity:** warning. **Fix:** safe: `rolling(20).max()` for min, max and median. unsafe: sum, mean, std and var. The `raw=True` case and a NumPy function have no fix.

Reports `rolling(20).apply(lambda w: w.max())`, `rolling(20).apply(f)` without `raw=True`, `rolling(20).agg(np.max)` and `rolling(20).apply(np.mean, raw=True)`.

Example:

```python
average = df["load"].rolling(20).apply(lambda window: window.mean())
```

Use instead:

```python
average = df["load"].rolling(20).mean()
```

#### text-time-key

**Severity:** warning. **Fix:** none.

Reports `df.groupby(df["d"].dt.strftime("%Y-%m"))`.

Example:

```python
monthly = df.groupby(df["time"].dt.strftime("%Y-%m"))["load"].sum()
```

Use instead:

```python
monthly = df.groupby(df["time"].dt.to_period("M"))["load"].sum()
```

The result has a PeriodIndex instead of text labels. Converting time zones to periods also removes the time zone.

The replacement is advice. xpd does not apply it automatically.

#### object-dates

**Severity:** warning. **Fix:** none.

Reports `df["d"].dt.date` and `dt.time`.

Example:

```python
days = df["time"].dt.date
```

Use instead:

```python
days = df["time"].dt.normalize()
```

normalize returns timestamps at midnight, not Python date objects. Keep dt.date when an external interface needs Python dates. For dt.time, choose the required datetime representation rather than normalizing away the time.

The replacement is advice. xpd does not apply it automatically.

#### apply-on-dates

**Severity:** warning. **Fix:** unsafe: `df["d"] + pd.Timedelta(days=7)`.

Reports `df["d"].apply(lambda t: t + pd.Timedelta(days=7))`, `t.replace(day=1)` and `t.strftime(...)`.

Example:

```python
next_week = df["time"].apply(lambda time: time + pd.Timedelta(days=7))
```

Use instead:

```python
next_week = df["time"] + pd.Timedelta(days=7)
```

#### strftime-date-part

**Severity:** warning. **Fix:** unsafe: `df["d"].dt.year`, `df["d"].dt.day_name()`.

Reports `df["d"].dt.strftime("%Y").astype(int)`, the same with `%m`, `%H` and `%j`, and `dt.strftime("%A")` and `"%B"`.

Example:

```python
year = df["time"].dt.strftime("%Y").astype(int)
weekday = df["time"].dt.strftime("%A")
```

Use instead:

```python
year = df["time"].dt.year
weekday = df["time"].dt.day_name()
```

#### apply-date-expression

**Severity:** warning. **Fix:** none.

Reports `df["d"].apply(lambda t: t.weekday() >= 5)` and `lambda t: t.hour * 60 + t.minute`, a lambda that compares a date part or computes with parts.

Example:

```python
weekend = df["time"].apply(lambda time: time.weekday() >= 5)
```

Use instead:

```python
weekend = df["time"].dt.weekday >= 5
```

The replacement is advice. xpd does not apply it automatically.

#### strftime-round-trip

**Severity:** warning. **Fix:** unsafe: `df["d"].dt.normalize()`, `df["d"].dt.floor("h")`. The comparison has no fix.

Reports `pd.to_datetime(df["d"].dt.strftime("%Y-%m-%d"))` and `df["d"].dt.strftime("%Y-%m-%d") == "2023-06-01"`, which turn dates into text to drop the time.

Example:

```python
day = pd.to_datetime(df["time"].dt.strftime("%Y-%m-%d"))
```

Use instead:

```python
day = df["time"].dt.normalize()
```

#### index-year-mask

**Severity:** warning. **Fix:** none.

Reports `df[df.index.year == 2023]`.

Example:

```python
year = df[df.index.year == 2023]
```

Use instead:

```python
year = df.loc["2023"]
```

Use the label example on a sorted DatetimeIndex. It is a label selection, not a rewrite for every kind of index.

The replacement is advice. xpd does not apply it automatically.

#### apply-date-method

**Severity:** warning. **Fix:** unsafe: `df["d"].dt.floor("h")`, `df["d"].dt.weekday`, `df["d"].dt.date`, `df["d"].dt.isocalendar().week`. `timestamp` and `toordinal` have no fix.

Reports `df["d"].apply(lambda t: t.floor("h"))`, and the same with `ceil`, `normalize`, `tz_localize`, `tz_convert`, `day_name`, `month_name`, `to_period`, `total_seconds`, `weekday`, `date`, `isocalendar`, `timestamp` and `toordinal`.

Example:

```python
hours = df["time"].apply(lambda time: time.floor("h"))
```

Use instead:

```python
hours = df["time"].dt.floor("h")
```

#### apply-parse-date

**Severity:** warning. **Fix:** unsafe: `pd.to_datetime(df["raw"], format="%Y-%m-%d")`.

Reports `df["raw"].apply(lambda v: datetime.strptime(v, "%Y-%m-%d"))`, and a lambda or `map` that calls `pd.to_datetime`, `pd.Timestamp` or `parser.parse` per value.

Example:

```python
when = df["raw"].apply(lambda text: datetime.strptime(text, "%Y-%m-%d"))
```

Use instead:

```python
when = pd.to_datetime(df["raw"], format="%Y-%m-%d")
```

#### timedelta-per-value

**Severity:** warning. **Fix:** unsafe: `pd.to_timedelta(df["n"], unit="D")` for `pd.Timedelta(v, unit="D")`. The other shapes have no fix.

Reports `df["n"].apply(lambda v: pd.Timedelta(days=v))`, the same with `timedelta` and `pd.DateOffset`, and `pd.to_timedelta(df["n"].astype(str) + " days")`.

Example:

```python
shift = df["days"].apply(lambda days: pd.Timedelta(days=days))
```

Use instead:

```python
shift = pd.to_timedelta(df["days"], unit="D")
```

The units are "W", "D", "h", "min", "s", "ms" and "us". On fractional values the two forms can differ in the last digits.

The `rolling-apply` fixes for sum, mean, std and var are unsafe, because the window methods keep
running totals, so values far apart in size can give another result. For a function with no
window method the message names `raw=True` and `engine="numba"`. `apply-on-dates` writes
`df["d"] - pd.to_timedelta(df["d"].dt.day - 1, unit="D")` for `replace(day=1)`, which keeps the
time of day. Its fixes are unsafe, because a missing date gives a missing result where the
lambda raised.

`strftime-date-part` was 33 to 198 times slower than the part on 1,000,000 rows. Its fixes are
unsafe: a part is int32 where `astype(int)` gave int64, and `day_name` is English where `strftime`
follows the locale. `apply-date-expression` was 22 to 94 times slower. It has no fix, because the
rewrite depends on the operator. `strftime-round-trip` was 18 to 202 times slower. Its fix is
unsafe, because on dates with a time zone the round trip drops the zone. For `timestamp` in a
lambda write `(df["d"] - pd.Timestamp("1970-01-01")) / pd.Timedelta("1s")`, which was 229 times
faster. `isocalendar().week` is UInt32 where the lambda gave int64. `rolling(60).agg(np.max)` was
1,031 times slower than `rolling(60).max()` on 200,000 rows.

The `apply-date-method` fix is unsafe, because the dtype can change: `dt.weekday` is int32 where
the lambda gave int64. It leaves `round` alone, because `lambda v: v.round(2)` is more often a
number. The `apply-parse-date` fix is unsafe for the reason `apply-known-function` gives, and
because `strptime` raises on a missing value where `pd.to_datetime` returns NaT. It reports
`strptime` only on `datetime`, because `time.strptime` returns another type. `timedelta-per-value`
has no fix for a keyword such as `days=v`, because the keyword must become a unit. It leaves
`pd.DateOffset(months=v)` alone, because a month has no fixed length. On 1,000,000 rows `dt.floor`
ran 1,354 times faster than the lambda, `pd.to_datetime` with a format 24 times faster than
`strptime`, and `pd.to_timedelta` 1,549 times faster.

### Stated intent

#### merge-without-validate

**Severity:** warning. **Fix:** none.

Reports `merge` without `validate=`, which can duplicate rows.

Example:

```python
joined = left.merge(right, on="key")
```

Use instead:

```python
joined = left.merge(right, on="key", validate="many_to_one")
```

Choose validate to match the data relationship. many_to_one is only the example, not a default xpd inserts. many_to_many does not check key uniqueness.

The replacement is advice. xpd does not apply it automatically.

#### merge-without-on

**Severity:** warning. **Fix:** none.

Reports `merge` without `on=`, which joins on every shared column name.

Example:

```python
joined = left.merge(right, validate="many_to_one")
```

Use instead:

```python
joined = left.merge(right, on="key", validate="many_to_one")
```

Choose the actual join keys. Omitting on makes every shared column name a key.

The replacement is advice. xpd does not apply it automatically.

#### to-csv-index

**Severity:** warning. **Fix:** none.

Reports `df.to_csv(path)` without `index=`.

Example:

```python
df.to_csv(path)
```

Use instead:

```python
df.to_csv(path, index=False)
```

The replacement is advice. xpd does not apply it automatically.


### Reading and writing files

#### read-csv-slow-option

**Severity:** warning. **Fix:** none.

Reports `read_csv(..., engine="python")`, `sep=None` and `low_memory=False`, which turn off reading on several threads.

Example:

```python
df = pd.read_csv(path, engine="python")
```

Use instead:

```python
df = pd.read_csv(path)
```

The example needs no Python-engine-only options. Keep engine="python" when the separator or skipfooter requires it. For low_memory=False, specify the required column dtypes rather than dropping it blindly.

The replacement is advice. xpd does not apply it automatically.



#### read-then-head

**Severity:** warning. **Fix:** unsafe: `nrows=1000`.

Reports `pd.read_csv(path).head(1000)` and `.iloc[:1000]`, which parse the whole file to keep the first rows.

Example:

```python
sample = pd.read_csv(path).head(1000)
```

Use instead:

```python
sample = pd.read_csv(path, nrows=1000)
```

#### read-csv-converters

**Severity:** warning. **Fix:** none.

Reports `read_csv(..., converters=...)`, which calls a Python function once per value.

Example:

```python
df = pd.read_csv(path, converters={"price": float, "name": str.strip})
```

Use instead:

```python
df = pd.read_csv(path, dtype={"price": float})
df["name"] = df["name"].str.strip()
```

The replacement is advice. xpd does not apply it automatically.

#### read-json-lines-engine

**Severity:** warning. **Fix:** unsafe: `engine="pyarrow"`. The `json.loads` loop has no fix.

Reports `pd.read_json(path, lines=True)` without an engine, and `pd.DataFrame([json.loads(line) for line in f])`.

Example:

```python
df = pd.read_json(path, lines=True)
```

Use instead:

```python
df = pd.read_json(path, lines=True, engine="pyarrow")
```

#### to-csv-append-header

**Severity:** error. **Fix:** none.

Reports `df.to_csv(path, mode="a")` without `header=`, which writes the header again on every call.

Example:

```python
for part in parts:
    part.to_csv(path, mode="a", index=False)
```

Use instead:

```python
for part in parts:
    part.to_csv(path, mode="a", index=False, header=not os.path.exists(path))
```

The repeated header rows are read back as data, so every number in the file comes back as text.

The replacement is advice. xpd does not apply it automatically.

`read-csv-slow-option` leaves `engine="python"` alone when the call needs it, for `skipfooter` or
a separator of several characters other than `\s+`. `sep=None` was 54 times slower than a named
separator on 1,000,000 rows. `nrows` was 58 times faster than `head`. Its fix is unsafe, because
pandas guesses the dtypes from the rows it reads. `converters` was 3.9 to 7.8 times slower than
`dtype` or a column method after the read. `read_json` with the pyarrow engine was 18 times faster,
and 27 times faster than the `json.loads` loop. Its fix is unsafe, because pyarrow can pick another
dtype for a column. A call that reads from a `StringIO` is left alone, because the pyarrow engine
raises on a text buffer.

### Packages

`xpd check` also looks at the project's venv once per run. When the venv has pandas, it warns
about each package below that the venv lacks. The warning points at the project's
`pyproject.toml`, or at the venv when there is none. A project without a venv gets no package
warning, and `xpd hook` never gives one.

#### missing-pyarrow

**Severity:** warning. **Fix:** none.

Text columns are stored as Python strings without it. The `.str` methods ran 3 to 11 times faster with it on 1,000,000 rows.

The warning appears when pyarrow is not installed.

For a project managed with uv:

```sh
uv add pyarrow
```

Add the package to the project environment, not to xpd itself.

#### missing-bottleneck

**Severity:** warning. **Fix:** none.

`std` ran 4 times faster and `median` 1.7 times faster with it on 10,000,000 rows.

The warning appears when bottleneck is not installed.

For a project managed with uv:

```sh
uv add bottleneck
```

Add the package to the project environment, not to xpd itself.

#### missing-pandas-stubs

**Severity:** warning. **Fix:** none.

Without the stubs a type checker does not check calls to pandas.

The warning appears when pandas-stubs is not installed, and the project uses mypy, pyright, basedpyright, ty or pyrefly.

For a project managed with uv:

```sh
uv add --dev pandas-stubs
```

Add the package to the project environment, not to xpd itself.


A project uses a type checker when the venv has it or `pyproject.toml` has a `[tool.<name>]`
table for it. numexpr and numba have no check. numexpr changed little in measurements, and numba
only helps a function passed to `rolling(...).apply`, where the `rolling-apply` message names it.

## scikit-learn

These rules run on files that import `sklearn`. Each old form gives a wrong or leaked score, or a
score that changes from run to run. Speed plays no part. The numbers are means of 30 seeds on
scikit-learn 1.9.1. On data with no signal, an honest accuracy is 0.5.

Several of these rules read the names of variables, such as `X_test` and `y_pred`, because that
is all a pattern can see. A name that misleads gives a wrong report.

### Leakage

#### fit-on-test

**Severity:** error. **Fix:** unsafe: `scaler.transform(X_test)`. `fit` on test data has no fix.

Reports `scaler.fit_transform(X_test)` and `model.fit(X_test, y_test)`, which learn from the test rows.

Example:

```python
X_train = scaler.fit_transform(X_train)
X_test = scaler.fit_transform(X_test)
```

Use instead:

```python
X_train = scaler.fit_transform(X_train)
X_test = scaler.transform(X_test)
```

#### fit-before-split

**Severity:** error. **Fix:** none.

Reports `X = selector.fit_transform(X, y)` followed by `train_test_split(X, y)`, and `fit_resample` before the split.

Example:

```python
X = selector.fit_transform(X, y)
X_train, X_test, y_train, y_test = train_test_split(X, y, random_state=0)
```

Use instead:

```python
X_train, X_test, y_train, y_test = train_test_split(X, y, random_state=0)
X_train = selector.fit_transform(X_train, y_train)
X_test = selector.transform(X_test)
```

The replacement is advice. xpd does not apply it automatically.

#### cross-val-on-fitted-data

**Severity:** error. **Fix:** none.

Reports `cross_val_score(model, scaler.fit_transform(X), y)`, and a fitted result passed to it from an earlier line.

Example:

```python
X_picked = selector.fit_transform(X, y)
scores = cross_val_score(model, X_picked, y, cv=5)
```

Use instead:

```python
pipeline = make_pipeline(selector, model)
scores = cross_val_score(pipeline, X, y, cv=5)
```

The replacement is advice. xpd does not apply it automatically.

#### get-dummies-on-test

**Severity:** error. **Fix:** none.

Reports `pd.get_dummies(X_test)`, which makes one column per value in the test rows.

Example:

```python
X_train = pd.get_dummies(X_train)
X_test = pd.get_dummies(X_test)
```

Use instead:

```python
encoder = OneHotEncoder(handle_unknown="ignore")
X_train = encoder.fit_transform(X_train)
X_test = encoder.transform(X_test)
```

The replacement is advice. xpd does not apply it automatically.


`fit-on-test` reads the first argument's name: `test`, `val`, `valid`, `validation` or `holdout`
for `fit_transform`, and `test` only for `fit`, because a fit on a validation set can be meant, as
in probability calibration. A scaler refitted on test rows that arrive 5 at a time scored 0.865
where `transform` scored 0.976. An encoder refitted on the test rows scored 0.795 against 0.905.
A model fitted on its test rows scored 1.000 on noise. The fix is unsafe, because a name can
mislead. `StandardScaler().fit_transform(X_test)` has no fix, because that scaler was never fitted
on the training data.

`fit-before-split` reports the fit when a later statement of the same block passes its result to
`train_test_split`. Feature selection before the split scored 0.781 on noise, where the honest
value was 0.503. SMOTE before the split gave an F1 of 0.842 on noise, where the honest value was
0.164. For `StandardScaler` the leak is real and small: 0.9673 against 0.9667. xpd cannot tell a
scaler from a selector, so it reports both. It leaves a label encoder on the target alone, when the
result is named `y`, `target` or `labels`, or the encoder's name holds `label`.

`cross-val-on-fitted-data` covers `cross_val_score`, `cross_validate`, `cross_val_predict`,
`learning_curve` and `validation_curve`. Cross-validated accuracy on noise was 0.781 where the
Pipeline gave 0.510. `GridSearchCV(...).fit(X_fitted, y)` leaks in the same way and is not
reported, because xpd cannot tell a search from any other object with `fit`.

`get-dummies-on-test` reads the first argument's name, as `fit-on-test` does. In 15 of 30 splits
`predict` raised on the columns. Passed on as arrays, 102 of 200 splits raised and 17 ran on the
wrong columns. A call followed by `reindex` or `align` is left alone.

### Metrics

#### auc-from-labels

**Severity:** error. **Fix:** none.

Reports `roc_auc_score(y_test, model.predict(X_test))`, which ranks class labels where the metric needs a score per row.

Example:

```python
auc = roc_auc_score(y_test, model.predict(X_test))
```

Use instead:

```python
auc = roc_auc_score(y_test, model.predict_proba(X_test)[:, 1])
```

The replacement is advice. xpd does not apply it automatically.

#### metric-arguments-swapped

**Severity:** error. **Fix:** unsafe: `precision_score(y_test, y_pred)`.

Reports `precision_score(y_pred, y_test)`, with the predictions first.

Example:

```python
precision = precision_score(y_pred, y_test)
```

Use instead:

```python
precision = precision_score(y_test, y_pred)
```


`auc-from-labels` covers `roc_auc_score`, `average_precision_score`, `roc_curve`,
`precision_recall_curve`, `log_loss` and `brier_score_loss`. The AUC was 0.755 from labels and
0.875 from probabilities. It also reports `predict_proba(X)[:, 0]`, which scored 0.126, and a
variable that holds the result of `predict` and reaches the metric on a later line. Files that
import keras, tensorflow or torch are left alone, because `predict` returns probabilities there.

`metric-arguments-swapped` covers `precision_score`, `recall_score`, `confusion_matrix`,
`classification_report`, `r2_score`, `explained_variance_score`, `mean_absolute_percentage_error`,
`balanced_accuracy_score` and `fbeta_score`. The swapped precision is the recall: 0.817 against
0.967. R2 went from 0.48 to -3.19. `accuracy_score` and `f1_score` return the same number either
way and are left alone. The rule reports when the first name holds `pred` or `hat` and the second
holds `true`, `test`, `val`, `actual`, `target` or `label`, or is `y`.

### Reproducible results

#### missing-random-state

**Severity:** warning. **Fix:** none.

Reports `train_test_split(X, y)`, `RandomForestClassifier()` and `KFold(shuffle=True)` without `random_state`.

Example:

```python
X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2)
model = RandomForestClassifier()
```

Use instead:

```python
X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2, random_state=0)
model = RandomForestClassifier(random_state=0)
```

The value 0 is only the example. Any fixed number makes the run repeatable.

The replacement is advice. xpd does not apply it automatically.


The same script, run 30 times, scored between 0.930 and 1.000. The rule names the classes whose
output changed between two runs: the random forests, extra trees, bagging and gradient boosting
models, `IsolationForest`, the decision trees, `KMeans`, `MiniBatchKMeans`, `GaussianMixture`, the
MLP and SGD models, `TruncatedSVD`, `NMF`, `TSNE`, `ShuffleSplit`, `StratifiedShuffleSplit` and
`RepeatedKFold`, and `KFold`, `StratifiedKFold`, `GroupKFold` and `StratifiedGroupKFold` with
`shuffle=True`. `LogisticRegression`, `SVC`, `LinearSVC` and `PCA` gave the same output twice and
are left alone. A call with `shuffle=False` or `**params` is left alone, and so is a file that
calls `np.random.seed`, because scikit-learn then draws from that seed.

## Hamilton

These rules run on files that import `hamilton`, the module of
[Apache Hamilton](https://hamilton.apache.org/), which PyPI ships as `apache-hamilton` and
`sf-hamilton`. Hamilton builds a dataflow from plain functions: the function name is the output
and the parameter names are its inputs. The results below are from sf-hamilton 1.90.0 with pandas
3.1.1.

A node module that uses no Hamilton decorator has no `hamilton` import, and xpd does not check
it.

### Shared inputs

#### node-mutates-input

**Severity:** error. **Fix:** none.

Reports a node that writes to one of its parameters, such as `raw["uk"] = ...`, `raw.loc[mask, "uk"] = 0`, `raw.columns = ...`, `del raw["dk"]`, `raw.pop("uk")` and `rows.append(...)`.

Example:

```python
def clipped(raw: pd.DataFrame) -> pd.DataFrame:
    raw.loc[raw["uk"] < 0, "uk"] = 0
    return raw


def total_uk(raw: pd.DataFrame) -> float:
    return float(raw["uk"].sum())
```

Use instead:

```python
def clipped(raw: pd.DataFrame) -> pd.DataFrame:
    return raw.assign(uk=raw["uk"].clip(lower=0))


def total_uk(raw: pd.DataFrame) -> float:
    return float(raw["uk"].sum())
```

The replacement is advice. xpd does not apply it automatically.

Hamilton calls each function once and hands its result, the same object, to every node that
takes it. With the example above, `execute(["total_uk"])` gave 2.0 and
`execute(["clipped", "total_uk"])` gave 4.0. With the replacement both gave 2.0. The Hamilton
documentation asks for the same under
[output immutability](https://hamilton.apache.org/concepts/best-practices/output-immutability/).

A node is a function at module level with a return hint and a name that does not start with an
underscore. The rule also reports a write through a second name for the input, as in
`gdf = buildings` followed by `gdf["area"] = ...`. It leaves these alone:

- A parameter the function rebinds first, as in `raw = raw.copy()`.
- A function under `@load_from` or `@dataloader`. No other node can take the frame they inject.
- A function under `@mutate` or `@hamilton_exclude`.
- `update`, `append` and the like when the result is used, as in `return state.update(row)`,
  because an immutable object returns a new one.
- `inplace=True`, which [no-inplace](#no-inplace) reports.

A write that reaches the input through a helper function is not seen.

## Python

These rules are about Python itself, so they run on every file, whatever it imports. They cover
style choices that no formatter or type checker makes. The `python-best-practices` skill in
[`skills/`](https://github.com/bjoernaagaard/xpd/tree/main/skills) holds the rest of the style,
and `python-review` reviews code against it.

### Names

#### module-private-name

**Severity:** warning. **Fix:** none.

Reports a function, class or variable at module level whose name starts with an underscore.

Example:

```python
_LIMIT = 100


def _prices(rows):
    return [row["price"] for row in rows][:_LIMIT]


def total(rows):
    return sum(_prices(rows))
```

Use instead:

```python
__all__ = ["total"]

LIMIT = 100


def prices(rows):
    return [row["price"] for row in rows][:LIMIT]


def total(rows):
    return sum(prices(rows))
```

The replacement is advice. xpd does not apply it automatically.

`__all__` says in one place which names callers may import, and `from module import *`, type
checkers and documentation tools all read it. An underscore says the same thing once per name
and makes every use of the name harder to read.

The rule leaves these alone:

- A method or attribute of a class, such as `def _load(self)`.
- A name inside a function.
- `_` alone, and names with two leading underscores such as `__all__` and `__version__`.
- A file that imports `hamilton`, where an underscore tells Hamilton that a function is not a
  node.

### Comprehensions

#### comprehension-merges-dicts

**Severity:** warning. **Fix:** none.

Reports a comprehension or generator expression that merges dicts with `**` for every item.

Example:

```python
def table(models, catalog):
    return [{"id": m.id, **owner(m), **limits(catalog, m)} for m in models]
```

Use instead:

```python
def table(models, catalog):
    rows = []
    for m in models:
        row = {"id": m.id}
        row.update(owner(m))
        row.update(limits(catalog, m))
        rows.append(row)
    return rows
```

The replacement is advice. xpd does not apply it automatically.

The comprehension builds a whole row in one expression, and every part of the row has to be a
helper that returns a dict. The loop reads from top to bottom, and a step in it can grow without
the row being rewritten. A comprehension that maps or filters one value per item is still the
right tool.

`{**d for d in dicts}`, which Python 3.15 added, is not reported.

#### unpacked-comprehension

**Severity:** warning. **Fix:** none.

Reports a dict literal that unpacks a dict comprehension with `**`.

Example:

```python
def row(model, limits):
    return {"id": model.id, **{f"limit_{k}": v for k, v in limits.items()}}
```

Use instead:

```python
def row(model, limits):
    out = {"id": model.id}
    for name, value in limits.items():
        out[f"limit_{name}"] = value
    return out
```

The replacement is advice. xpd does not apply it automatically.

The literal builds a dict, unpacks it and merges it in one expression, so a reader has to run
the comprehension in their head to know which keys the result has.

## Write a rule

Rules are [ast-grep rules](https://ast-grep.github.io/guide/rule-config.html) in `.yml` files,
compiled into the binary. A pack lives at `rules/<module>/`, where `<module>` is the name you
import.

```text
rules/pandas/
├── pack.yml        # the pack's manifest: import aliases and package checks. A pack may leave it out
├── row-loop.yml    # one file per rule, named after the rule id
└── ...
```

A rule file holds the documents of one rule, one per shape, and each document has the file's name
as its `id`. The build fails on any other id.

A rule may have one ast-grep `fix:`. It must then say whether the fix keeps what the code does,
with `metadata: { fix: safe }` or `metadata: { fix: unsafe }`. A fix may write an alias that
`pack.yml` lists under `aliases`, such as `pd.`, and xpd applies it only in a file that imports
the module under that alias.

A rule that needs a newer library version says so with `metadata: { since: "3.1" }`. xpd skips it
when the project's venv holds an older version of the pack's library. It finds the version for any
pack the same way: from the dist-info folder whose name is the module, or whose `top_level.txt`
lists the module.

Add a file, rebuild, and check it against a sample:

```sh
cargo build --release
target/release/xpd check samples/bad.py
cp samples/bad.py /tmp/bad.py && target/release/xpd check /tmp/bad.py --fix --unsafe-fixes
```

The files in `samples/pandas/`, `samples/sklearn/`, `samples/hamilton/` and `samples/python/` mark each line a rule
reports with a comment that names the rules.

`pack.yml` may list package checks under `packages`. Each entry has an `id`, the `package` to
look for, a `message`, and optionally `onlyWith`, a list of tools. xpd warns when the venv has the
pack's library and lacks the package. With `onlyWith`, it warns only when the project also uses
one of the tools.

A new folder under `rules/` is a new pack, with no Rust change. Its rules run only on files that
import the module. A pack about Python itself, such as `rules/python/`, sets `everyFile: true` in
`pack.yml`, and its rules run on every file.
