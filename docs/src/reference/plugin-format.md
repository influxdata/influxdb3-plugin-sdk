# The Plugin Directory Format

A plugin is a directory containing a `manifest.toml` and the Python source that
implements the triggers the manifest declares. This page specifies the
directory-layout contract the SDK validates: which file is the entry point, and
how declared triggers bind to Python functions.

This is a format contract in the same sense as the [manifest](./manifest.md)
and [registry index](./registry-index.md) formats: every consumer — the CLI
that packages a plugin and the runtime that loads it — must agree on it.

## Required files

- `manifest.toml` at the plugin root is **required**. See
  [The Manifest Format](./manifest.md).
- A Python entry point at the plugin root (see below) is **required**.

## Entry-point detection

The entry point is determined from the **top-level regular files** of the
plugin directory. Subdirectories are not searched, and symbolic links are
excluded.

- **Multi-file plugin** — the root contains `__init__.py`. That file is the
  entry point; any number of helper modules may sit alongside it. `__init__.py`
  takes priority over any other top-level `.py` file.
- **Single-file plugin** — the root contains no `__init__.py` and exactly one
  top-level `.py` file. That file is the entry point.
- **No entry point** — the root contains no top-level regular `.py` file. This
  is a validation error.
- **Ambiguous** — the root contains no `__init__.py` but two or more top-level
  `.py` files. This is a validation error; add `__init__.py` to declare a
  multi-file plugin, or keep only one `.py` file.

Detection rules:

- Non-`.py` files (for example `requirements.txt`, `README.md`) are ignored
  for entry-point detection.
- Symbolic links are excluded (archives store the link target, not the link).
- Subdirectories are ignored; nested `.py` files (for example
  `pkg/helper.py`) do not count, and a *directory* named `foo.py` is not an
  entry point.
- Matching is case-sensitive: `Foo.PY` and `__INIT__.py` are not treated as
  `foo.py`/`__init__.py`.

**Interaction with [`[plugin].exclude`](./manifest.md#pluginexclude).** Source-file
selection applies `exclude` patterns *before* entry-point detection runs. Only
the files that survive selection are considered when classifying the entry point;
excluded top-level `.py` files do not count. This means `exclude` can remove
what would otherwise be the sole entry point, yielding the ordinary "no entry
point" validation error.

## Trigger binding

Each trigger declared in `manifest.toml`'s `plugin.triggers` must be
implemented as a **top-level synchronous** `def <trigger>(...)` in the entry
point.

- A top-level `def <trigger>(...)` satisfies the trigger.
- A top-level decorated function (`@deco` then `def <trigger>`) counts — a
  decorator is not indirection.
- An `async def <trigger>(...)` is rejected: the runtime invokes trigger
  functions synchronously.
- A definition that is **not** top-level does not count: class methods, nested
  defs, defs guarded by `if`/`try`, re-exports, and module-level assignments
  (`<trigger> = something`) all fail to bind.
- If a name is defined more than once at the top level, the **last** definition
  wins (mirroring Python's own rebind semantics).
- The entry-point source must parse as valid Python 3; a parse error is
  reported and no trigger checks run.

## Diagnostics

Validation collects every diagnostic it can safely gather in one pass, so an
entry-point problem and a manifest problem (for example) are reported together
rather than one at a time. A manifest that fails to parse stops the cross-file
trigger checks — the set of declared triggers is unknown without a valid
manifest — but any entry-point diagnostic already found is still reported
alongside the manifest errors.

---

Back: [The Manifest Format](./manifest.md) | Next: [The Registry Index Format](./registry-index.md)
