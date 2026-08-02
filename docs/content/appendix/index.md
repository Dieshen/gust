---
title: "Appendix"
description: "Reference material for lookup: the complete grammar, the standard library, limitations, answers, and release history."
type: reference
---

# Appendix

Material you look things up in rather than read through. Where the [Reference](../reference/index.md) section explains what each language feature means, the appendix records exactly what exists — the productions, the machine signatures, and the edges the compiler currently stops at.

## Pages

::: grids
    ::: grid
        ::: card "Grammar" icon:braces
        Every production in `grammar.pest`, grouped by area, plus the constraints that fall out of it — no loops, no method calls, no string escapes.

        [Read →](grammar.md)
        :::
    :::
    ::: grid
        ::: card "Stdlib API" icon:library
        The six machines and one type in `gust-stdlib`: states, transitions, and the effects you must implement for each.

        [Read →](stdlib_api.md)
        :::
    :::
:::

::: grids
    ::: grid
        ::: card "FAQ" icon:help-circle
        Short answers to the questions Gust raises most: why valid-looking Rust fails to parse, what `ctx` is, when to use `action`.

        [Read →](faq.md)
        :::
    :::
    ::: grid
        ::: card "Known Limitations" icon:triangle-alert
        Where the language and each backend currently stop, and what to do about it. Read before choosing a target.

        [Read →](known_limitations.md)
        :::
    :::
:::

::: grids
    ::: grid
        ::: card "Changelog" icon:history
        Release history, mirroring the repository's `CHANGELOG.md`.

        [Read →](changelog.md)
        :::
    :::
:::

## Where to start

- **A form will not parse.** [Grammar](grammar.md) — the "what `primary` does not contain" table covers most of it.
- **Generated code will not compile.** [Known Limitations](known_limitations.md#backends) — `gust check` validates the source, not the output.
- **You want a circuit breaker, a saga, or a retry policy.** [Stdlib API](stdlib_api.md) for the signatures, [Cookbook](../cookbook/index.md) for worked implementations.
- **Something changed between releases.** [Changelog](changelog.md).
