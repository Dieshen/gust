---
title: "Grammar"
description: "The complete Gust grammar, transcribed from the compiler's PEG, with the constraints that fall out of it."
type: reference
---

# Grammar

Gust's syntax is defined by a single [pest](https://pest.rs) PEG at `gust-lang/src/grammar.pest`. That file is the authority: the parser is generated from it, so anything it does not describe is a parse error, no matter how reasonable it looks in Rust.

This page transcribes every production in that file, grouped by area, and then draws out the constraints the grammar implies. Read it when you need to know whether a form exists at all. For what the forms *mean*, see the [Syntax reference](../reference/syntax.md).

## Notation

pest is a parsing expression grammar, not a context-free one. Alternation is **ordered** — `a | b` tries `a` first and only falls back to `b` if `a` fails — which matters in a couple of places noted below.

| Operator | Meaning |
| --- | --- |
| `~` | Sequence: match the left, then the right |
| `\|` | Ordered choice: try the left, then the right |
| `?` | Optional — zero or one |
| `*` | Zero or more |
| `+` | One or more |
| `!a` | Negative lookahead: succeeds if `a` does *not* match |
| `@` | Atomic rule — no implicit whitespace inside |
| `_` | Silent rule — matched but not kept in the parse tree |

Rules marked `@` (`ident`, `int_lit`, `float_lit`) admit no whitespace between their parts. Every other rule allows whitespace and comments freely between elements, because `WHITESPACE` and `COMMENT` are declared silent.

## Lexical structure

```text
WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
COMMENT    = _{ "//" ~ (!"\n" ~ ANY)* }
```

Comments are **line comments only**. There is no `/* ... */` form — a block comment is a parse error at the first `/`, not silently skipped.

## Identifiers

```text
ident = @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
```

ASCII only. No Unicode identifiers, no raw-identifier escape.

## Literals

```text
literal     = { string_lit | float_lit | int_lit | bool_lit }
string_lit  = { "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
float_lit   = @{ ASCII_DIGIT+ ~ "." ~ ASCII_DIGIT+ }
int_lit     = @{ ASCII_DIGIT+ }
bool_lit    = { "true" | "false" }
```

Three consequences that catch people out:

- **String literals have no escape sequences.** The body is "any character that is not a quote", so the first `"` ends the literal. `"he said \"hi\""` does not parse, and `"\n"` is a backslash followed by the letter `n`, not a newline.
- **Floats need digits on both sides of the point.** `0.5` is a float; `.5` and `5.` are not.
- **Integers are bare decimal digits.** No `1_000`, no `0xff`, no `10i64` suffix. A leading `-` is the unary operator applied to a positive literal, not part of the literal.

`float_lit` is tried before `int_lit`, so `1.5` lexes as one float rather than an integer followed by a stray point.

## Program structure

```text
program  = { SOI ~ (use_decl | type_decl | channel_decl | machine_decl)* ~ EOI }
use_decl = { "use" ~ path ~ ";" }
path     = { ident ~ ("::" ~ ident)* }
```

A file is a flat sequence of four top-level forms in any order and any number, including zero. There is no module, no `pub`, and no nesting: machines cannot contain machines, and types cannot be declared inside a machine.

Note the semicolon asymmetry. `use` ends with `;`. `type`, `enum`, `channel`, and `machine` do not.

## Type declarations

```text
type_decl    = { struct_decl | enum_decl }
struct_decl  = { "type" ~ ident ~ "{" ~ field_list ~ "}" }
enum_decl    = { "enum" ~ ident ~ "{" ~ variant_list ~ "}" }
variant_list = { (variant ~ ("," ~ variant)* ~ ","?)? }
variant      = { ident ~ ("(" ~ type_expr ~ ("," ~ type_expr)* ~ ","? ~ ")")? }
field_list   = { (field ~ ("," ~ field)* ~ ","?)? }
field        = { ident ~ ":" ~ type_expr }
```

Structs use the keyword `type`, not `struct`. Both lists may be empty and both accept a trailing comma.

**Enum variant payloads are positional.** `variant` takes a parenthesised list of *type expressions* — there is no `ident ":" type_expr` form inside it, so `Bar(String, i64)` parses and `Bar(name: String)` does not. This is why `EngineFailure` in the standard library documents its payload positions in a comment rather than in the type.

## Type expressions

```text
type_expr    = { unit_type | tuple_type | generic_type | simple_type }
unit_type    = { "()" }
tuple_type   = { "(" ~ type_expr ~ ("," ~ type_expr)+ ~ ","? ~ ")" }
generic_type = { ident ~ "<" ~ type_expr ~ ("," ~ type_expr)* ~ ">" }
simple_type  = { ident }
```

`tuple_type` requires the `+` — a one-element parenthesised type is not a tuple and does not parse. `generic_type` takes no trailing comma, unlike almost every other list in the grammar.

Type names are not checked here; `simple_type` is any identifier. Whether `i64` or `Vec` means anything is decided later, by the validator and the backend.

## Channels

```text
channel_decl        = { "channel" ~ ident ~ ":" ~ type_expr ~ ("(" ~ channel_config ~ ")")? }
channel_config      = { channel_config_item ~ ("," ~ channel_config_item)* ~ ","? }
channel_config_item = { capacity_item | mode_item }
capacity_item       = { "capacity" ~ ":" ~ int_lit }
mode_item           = { "mode" ~ ":" ~ channel_mode }
channel_mode        = { "broadcast" | "mpsc" }
```

**A channel declaration takes no trailing semicolon.** It is the one top-level form that ends with neither `}` nor `;`, so a reflexive `channel Events: Msg;` fails to parse on the stray `;`.

`capacity` takes an `int_lit` directly, so it cannot be an expression or a negative number.

```gust
type Point { x: i64, y: i64 }

channel Events: Point (capacity: 16, mode: mpsc)

machine Emitter (sends Events) {
    state Start(p: Point)
    state Done

    transition go: Start -> Done

    on go(ctx) {
        send Events(ctx.p);
        goto Done;
    }
}
```

## Machines

```text
machine_decl          = { "machine" ~ ident ~ generic_params? ~ machine_annotations? ~ ("{" ~ machine_body ~ "}") }
generic_params        = { "<" ~ generic_param ~ ("," ~ generic_param)* ~ ">" }
generic_param         = { ident ~ (":" ~ trait_bounds)? }
trait_bounds          = { ident ~ ("+" ~ ident)* }
machine_annotations   = { "(" ~ machine_annotation ~ ("," ~ machine_annotation)* ~ ","? ~ ")" }
machine_annotation    = { sends_annotation | receives_annotation | supervises_annotation }
sends_annotation      = { "sends" ~ ident }
receives_annotation   = { "receives" ~ ident }
supervises_annotation = { "supervises" ~ ident ~ ("(" ~ supervision_strategy ~ ")")? }
supervision_strategy  = { "one_for_one" | "one_for_all" | "rest_for_one" }
machine_body          = { machine_item* }
machine_item          = { state_decl | transition_decl | on_handler | effect_decl | action_decl }
```

Generic parameters come first, annotations second, then the body. Neither is required, and the ordering is fixed.

`trait_bounds` is a `+`-separated list of bare identifiers, so `T: Clone + Send` parses but `T: Into<String>` does not — a bound cannot itself be generic.

`machine_body` is a flat `machine_item*`: states, transitions, handlers, effects, and actions may appear in any order and be interleaved freely. The grammar imposes no grouping; the formatter does.

## States

```text
state_decl = { "state" ~ ident ~ ("(" ~ field_list ~ ")")? }
```

The field list is optional as a whole. `state Idle` and `state Idle()` are both valid, and a state with no fields is the common case.

## Transitions

```text
transition_decl = { "transition" ~ ident ~ ":" ~ ident ~ "->" ~ target_states ~ timeout_spec? }
target_states   = { ident ~ ("|" ~ ident)* }
timeout_spec    = { "timeout" ~ duration }
duration        = { int_lit ~ duration_unit }
duration_unit   = { "ms" | "s" | "m" | "h" }
```

Exactly one source state, one or more `|`-separated targets, and no trailing comma in the target list.

A duration is an integer immediately followed by a unit — `30s`, `500ms`, `2h`. `ms` is listed before `s` in the ordered choice, so `500ms` reads as milliseconds rather than as `500m` followed by a stray `s`. There is no fractional duration, because `duration` takes `int_lit` rather than any literal.

## Effects and actions

```text
async_modifier = { "async" }
effect_decl    = { async_modifier? ~ "effect" ~ ident ~ "(" ~ field_list ~ ")" ~ "->" ~ type_expr }
action_decl    = { async_modifier? ~ "action" ~ ident ~ "(" ~ field_list ~ ")" ~ "->" ~ type_expr }
```

The two productions are identical except for the keyword. `effect` marks a replay-safe, idempotent operation; `action` marks one that is externally visible and must not be replayed. Both lower the same way — the distinction is recorded for replay-aware runtimes.

**The return type is mandatory.** There is no `?` on `"->" ~ type_expr`, so an effect that returns nothing must say so: `effect log(msg: String) -> ()`.

Neither declaration has a body. An effect is a signature; you implement it in Rust or Go.

## Handlers

```text
on_handler = { async_modifier? ~ "on" ~ ident ~ "(" ~ param_list ~ ")" ~ ("->" ~ type_expr)? ~ block }
param_list = { (param ~ ("," ~ param)* ~ ","?)? }
param      = { ident ~ ":" ~ type_expr }
```

The handler name must match a declared transition. The parameter list may be empty.

The grammar admits a handler return type, but the compiler does not: the validator rejects it with *"handler return types are not yet supported"*. This is one of the few places where the grammar is deliberately looser than the implementation.

## Blocks and statements

```text
block     = { "{" ~ statement* ~ "}" }
statement = { let_stmt | return_stmt | if_stmt | match_stmt | transition_stmt
            | effect_stmt | send_stmt | spawn_stmt | expr_stmt }

let_stmt        = { "let" ~ ident ~ (":" ~ type_expr)? ~ "=" ~ expr ~ ";" }
return_stmt     = { "return" ~ expr ~ ";" }
transition_stmt = { "goto" ~ ident ~ ("(" ~ expr_list ~ ")")? ~ ";" }
effect_stmt     = { "perform" ~ ident ~ "(" ~ expr_list ~ ")" ~ ";" }
send_stmt       = { "send" ~ ident ~ "(" ~ expr ~ ")" ~ ";" }
spawn_stmt      = { "spawn" ~ ident ~ "(" ~ expr_list ~ ")" ~ ";" }
if_stmt         = { "if" ~ expr ~ block ~ ("else" ~ (if_stmt | block))? }
match_stmt      = { "match" ~ expr ~ "{" ~ match_arm* ~ "}" }
match_arm       = { pattern ~ "=>" ~ block }
expr_stmt       = { expr ~ ";" }
```

**There are nine statement forms and none of them is a loop.** No `for`, no `while`, no `loop`, no `break`, no `continue`. Iteration is modelled either as a self-transition carrying an index, or pushed into an effect.

Other details worth reading off directly:

- `let` always has an initialiser, and there is no `mut`. Every binding is single-assignment.
- `goto`'s argument list is optional: `goto Done;` and `goto Done(a, b);` are both valid.
- `send` takes exactly one expression — one message, not a list.
- `if` needs no parentheses around its condition, and `else if` chains via the `if_stmt` alternative.
- `if` has no expression form. It is a statement; you cannot write `let x = if c { ... }`.

### Match arms

```text
match_arm        = { pattern ~ "=>" ~ block }
pattern          = { wildcard_pattern | variant_pattern }
wildcard_pattern = { "_" }
variant_pattern  = { ident ~ ("::" ~ ident)? ~ ("(" ~ ident_list ~ ")")? }
ident_list       = { ident ~ ("," ~ ident)* ~ ","? }
```

**Match arms take a block and no separating comma.** `match_stmt` is `match_arm*` — nothing sits between arms — so `Ok(v) => v,` fails twice over: on the expression body and on the trailing comma.

Patterns are shallow. A `variant_pattern` binds a flat `ident_list`, so:

- `Ok(value)`, `Err(err)`, `Status::Active`, and `_` all parse.
- Literal patterns (`0 =>`), nested patterns (`Ok(Some(x))`), struct patterns, `|`-alternatives, bindings with `@`, and guards (`if cond` after the pattern) do not exist.

Because a payload is an `ident_list`, arity is positional and every binding is a plain name.

## Expressions

```text
expr       = { or_expr }
or_expr    = { and_expr ~ ("||" ~ and_expr)* }
and_expr   = { cmp_expr ~ ("&&" ~ cmp_expr)* }
cmp_expr   = { add_expr ~ (cmp_op ~ add_expr)? }
add_expr   = { mul_expr ~ (add_op ~ mul_expr)* }
mul_expr   = { unary_expr ~ (mul_op ~ unary_expr)* }
unary_expr = { unary_op? ~ primary }

perform_expr   = { "perform" ~ ident ~ "(" ~ expr_list ~ ")" }
qualified_path = { ident ~ "::" ~ ident }
primary        = { literal | perform_expr | qualified_path | field_access | fn_call | ident_expr | "(" ~ expr ~ ")" }
fn_call        = { ident ~ "(" ~ expr_list ~ ")" }
field_access   = { ident ~ ("." ~ ident)+ }
ident_expr     = { ident }
expr_list      = { (expr ~ ("," ~ expr)* ~ ","?)? }
```

Precedence runs from loosest to tightest down the chain: `||`, then `&&`, then comparison, then `+ -`, then `* / %`, then unary, then `primary`. Binary levels are left-associative.

`cmp_expr` ends in `?`, not `*`. **Comparison is non-associative: exactly one per expression.** `a < b < c` is a parse error, not a bad-typed comparison.

`perform` sits in `primary`, which is what makes it an expression as well as a statement — `let x = perform f(y);` and `goto S(perform g());` both work.

`field_access` requires the `+`, so it is one or more dots: `ctx.order.total` is fine, and a bare `ctx` falls through to `ident_expr`.

### What `primary` does not contain {#absent-expression-forms}

The ordered choice in `primary` is the whole expression vocabulary, and several familiar forms are simply absent:

| Absent form | Why the grammar rejects it |
| --- | --- |
| Method calls — `items.len()` | `field_access` is dots and identifiers with no call suffix |
| Struct literals — `Order { id: x }` | No production pairs an identifier with a braced field list |
| Enum construction with a payload — `Failure::Timeout(500)` | `qualified_path` is two identifiers with no argument list; it is tried before `fn_call`, so the `(500)` is left dangling |
| Tuple values — `(a, b)` | The parenthesised alternative holds a single `expr`; `tuple_type` exists only as a *type* |
| Indexing — `xs[0]` | No index production |
| `&`, `&mut`, `*` | `unary_op` is `!` and `-` only |
| `?`, `as`, ranges, closures | No production |
| Qualified calls — `Foo::bar(x)` | Same ordered-choice collision as enum construction |

Payload-carrying enum values and any computation the vocabulary above cannot express are built in an effect and returned:

```gust
enum Failure {
    Timeout(i64),
    Cancelled(String),
}

machine Job {
    state Running(elapsed_ms: i64)
    state Aborted(failure: Failure)

    transition give_up: Running -> Aborted

    effect timeout_failure(elapsed_ms: i64) -> Failure

    on give_up(ctx) {
        goto Aborted(perform timeout_failure(ctx.elapsed_ms));
    }
}
```

## Operators

```text
cmp_op   = { "==" | "!=" | "<=" | ">=" | "<" | ">" }
add_op   = { "+" | "-" }
mul_op   = { "*" | "/" | "%" }
unary_op = { "!" | "-" }
```

That is the complete operator set. There are no bitwise operators (`& | ^ << >>`), no compound assignment (`+=`), and no assignment operator at all — `let` is the only way to bind a name.

The two-character comparisons are listed before their one-character prefixes, so `<=` never lexes as `<` followed by `=`.

## Trailing commas

The grammar is inconsistent about trailing commas, so it is worth having the list to hand.

| Accepts a trailing comma | Does not |
| --- | --- |
| `field_list` (struct fields, state fields, effect and handler params) | `generic_params` — `machine M<T,>` |
| `variant_list` and a variant's payload types | `generic_type` arguments — `Vec<T,>` |
| `tuple_type` | `target_states` — `A \| B,` |
| `machine_annotations`, `channel_config` | `trait_bounds` |
| `expr_list` (calls, `perform`, `goto`, `spawn`) | `path` in a `use` |
| `ident_list` (pattern bindings) | |

## Where the grammar is looser than the compiler

Parsing is the first gate, not the last. Two forms parse and are then rejected downstream:

- **Handler return types.** `on go() -> i64 { ... }` parses; the validator reports *"handler return types are not yet supported"*.
- **Undeclared type names.** `simple_type` accepts any identifier, so a misspelled type reaches codegen as a bare name rather than being caught. Until 1.0 this was worse: an unrecognised type in a handler parameter marked that parameter as the from-state accessor and dropped it from the generated method. The accessor is now the parameter with **no** annotation (`param = { ident ~ (":" ~ type_expr)? }`), so a typo can no longer delete a parameter. See [Effects and Handlers](../reference/effects_handlers.md).

Conversely, `gust check` passing does not mean the generated code compiles. See [Known Limitations](known_limitations.md) for the constructs that validate cleanly and then fail a particular backend.

## Next steps

- [Syntax reference](../reference/syntax.md) — what each form means and how to use it
- [Stdlib API](stdlib_api.md) — real machines exercising most of this grammar
- [Known Limitations](known_limitations.md) — where the language and the backends currently stop
