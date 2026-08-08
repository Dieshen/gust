# Skills

Claude Code skills for Gust, kept in the repository so they are versioned,
reviewable, and — for the parts that are Gust source — tested.

| Skill | For |
|---|---|
| `gust/` | Writing `.gu` files and integrating generated output into a host project |
| `gust-dev/` | Working on the compiler in this workspace |

## Why they live here

They used to live only in `~/.claude/skills/`, outside any repository. Nothing
reviewed them and nothing tested them, so they drifted a full release behind:
teaching the pre-1.0 `ctx: SomeCtx` spelling that 1.0 rejects, listing five
codegen backends when two had been deleted, and pointing at test files that no
longer existed.

## What is tested

`gust-lang/tests/docs_snippets.rs` compiles every ` ```gust ` block here that is
a whole program — it must parse, validate, and generate for both backends.

Blocks that teach a *form* rather than a program are skipped, and the count of
each is printed. A block is treated as a fragment if it does not begin with a
top-level keyword, or if it contains `...` — the notation the syntax reference
uses for an elided body. Prose is not tested; it still needs review.

## Installing

Skills are loaded from `~/.claude/skills/`. Copy or symlink:

```bash
# Windows (PowerShell, as administrator for the symlink form)
New-Item -ItemType SymbolicLink -Path "$env:USERPROFILE\.claude\skills\gust" -Target "D:\Dev\rust\gust\skills\gust"
New-Item -ItemType SymbolicLink -Path "$env:USERPROFILE\.claude\skills\gust-dev" -Target "D:\Dev\rust\gust\skills\gust-dev"

# POSIX
ln -s "$PWD/skills/gust"     ~/.claude/skills/gust
ln -s "$PWD/skills/gust-dev" ~/.claude/skills/gust-dev
```

A symlink is preferable to a copy: with a copy, the two diverge again, which is
the problem this directory exists to solve.
