"""Style that xpd reports in any Python file, and the same code without it.

A line that xpd reports ends with a comment that names the rule.
"""

from functools import cache

__all__ = ["rows", "rows_with_loop"]

_LIMIT = 10  # xpd: module-private-name
LIMIT = 10
_ = LIMIT


def _prices(item: dict) -> dict:  # xpd: module-private-name
    return {"price": item["price"]}


@cache
def _cached() -> int:  # xpd: module-private-name
    return LIMIT


class _Row:  # xpd: module-private-name
    def _total(self) -> int:
        return 0


class Row:
    _count = 0

    def _total(self) -> int:
        _local = 1
        return _local


def prices(item: dict) -> dict:
    def _inner() -> dict:
        return {"price": item["price"]}

    return _inner()


def rows(items: list[dict]) -> dict:
    return {  # xpd: comprehension-merges-dicts
        item["id"]: {"name": item["name"], **prices(item)} for item in items
    }


def flat(item: dict) -> dict:
    return {
        "id": item["id"],
        # xpd reports the next line as unpacked-comprehension.
        **{f"tag_{name}": value for name, value in item["tags"].items()},
    }


def merged(item: dict, extra: dict) -> dict:
    return {"id": item["id"], **extra}


def names(items: list[dict]) -> list[str]:
    return [item["name"] for item in items if item["name"]]


def rows_with_loop(items: list[dict]) -> dict:
    found = {}
    for item in items:
        row = {"name": item["name"]}
        row.update(prices(item))
        for name, value in item["tags"].items():
            row[f"tag_{name}"] = value
        found[item["id"]] = row
    return found
