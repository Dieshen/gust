# The validator

`gust-lang/src/validator.rs` is the largest file in the compiler (~76k). Entry point:

```rust
pub fn validate_program(program: &Program, file: &str, _source: &str) -> ValidationReport
```

`_source` is unused — spans on AST nodes carry positions. The report holds `errors: Vec<GustError>` and `warnings: Vec<GustWarning>`.

## Diagnostic anatomy

Both `GustError` and `GustWarning` carry `file`, `line`, `col`, `message`, and optional `note` and `help`. They render in rustc's visual style with a source-annotated caret block via `.render(source)`.

The three-part convention is what makes the diagnostics pleasant, and it's worth following precisely:

| Field | Answers | Example |
|---|---|---|
| `message` | What is wrong | `unknown effect 'proess'` |
| `note` | Why, or which rule | `goto argument count must match target state fields` |
| `help` | What to do | `did you mean 'process'?` |

Did-you-mean suggestions use `strsim` for fuzzy matching against declared names. When adding a check over a namespace of user-declared identifiers, add the suggestion — it's the difference between a diagnostic that stops someone and one that fixes their problem.

## Error or warning?

Errors block compilation; warnings don't. The decision has real consequences, and the guiding question is *whether the code is definitely wrong for every backend*.

The unused-`let` check is the instructive case. It's a **warning**, because Rust merely warns — but Go rejects an unused local outright, so ignoring it produces a Go package that will not build. It's reported against the `.gu` so the author hears it once, at the source, rather than as a backend-specific surprise later (issue #100). That's the pattern: when backends disagree about severity, warn at the Gust level rather than picking one backend's rule.

## Existing checks

| Function | Checks |
|---|---|
| `validate_goto_arity` | `goto` argument count matches target state field count |
| `validate_goto_targets` | `goto` names a state the transition actually declares as a target |
| `validate_goto_types` | Argument types match target state field types |
| `validate_perform_arity` | `perform` argument count matches the effect declaration |
| `validate_expression_types` | Operand types in binops, let annotations |
| `validate_unused_let_bindings` | Bindings never read (see above) |
| `validate_shadowed_handler_params` | A handler param shadowing a source-state field |
| `validate_ctx_field_access` | `ctx.foo` resolves to a real source-state field |
| `validate_send_targets` | `send` names a declared channel |
| `validate_spawn_targets` | `spawn` names a declared machine |

Plus unused-effect warnings, unreachable-state detection (driven by an incoming-transition counter), and the `action` rules: at most one action per code path, and the action must be the last side-effectful step before the `goto`.

## The type-inference contract

`TypeContext` infers expression types for goto-argument, let-annotation, and binop-operand checks. It is **deliberately conservative**: unknown types — a plain function call's return, a generic parameter, an undeclared type name — are treated as "skip the check" rather than reported.

Preserve this bias. A false positive in a validator is far more costly than a missed check: it blocks correct code and trains people to distrust the tool. Generic type parameters are treated as compatible with any type for the same reason.

Before 1.0 this was also why undeclared type names passed silently, which was load-bearing — the old `ctx: SomeCtx` idiom depended on `SomeCtx` being undeclared. Both are gone: the accessor is now the parameter with no type annotation, and an undeclared type name is a hard error.

## The ctx resolution rule

`detect_ctx_param` in `codegen_common.rs` is shared by the validator and every backend, and getting it wrong changes the generated API:

```rust
pub fn detect_ctx_param(handler: &OnHandler, known_types: &HashSet<String>) -> Option<String> {
    // 1. First param whose type is a Simple type NOT in known_types.
    // 2. Otherwise, if the body references `ctx`, assume a param named "ctx".
}
```

That parameter is dropped from the generated method signature, and its field accesses resolve to source-state fields. Consequences to keep in mind when touching this:

- **A misspelled type on the first parameter silently makes it the ctx accessor** and removes it from the signature. This is the most confusing failure mode in the language.
- The validator special-cases the ctx parameter when type-checking goto arguments, since its fields resolve through the from-state rather than its nominal type.
- Any change here must land identically across the shipped backends or the same `.gu` yields different APIs per target.

## Spans

Only top-level nodes carry real spans — declarations, `goto`, `perform`, `send`, `spawn`. **Expression nodes fall back to default spans**, so a diagnostic anchored to a subexpression points at the wrong place. Tracked in issue #46.

Don't build a check whose value depends on precise expression positions without fixing spans first; the diagnostic will be actively misleading, which is worse than not having it.

## Adding a check

1. Write the function taking what it needs plus `file: &str` and `report: &mut ValidationReport`; follow the `validate_*` naming.
2. Call it from `validate_program`, inside the per-machine or per-handler loop as appropriate.
3. Decide error vs warning using the backend-disagreement test above.
4. Fill in all three of `message` / `note` / `help`. Add a `strsim` suggestion if the check is over user-declared names.
5. Add cases to `diagnostics_validation.rs` — both the firing case and a near-miss that must **not** fire. The second matters more; conservatism is the design goal.
6. Consider running `cargo mutants --file gust-lang/src/validator.rs` afterward. It's the highest-value file for mutation testing, and a new branch is exactly where survivors show up.
