---
title: "Syntax"
description: "Every declaration, statement, and expression form the Gust grammar accepts, and the Rust-looking forms it rejects."
type: reference
---

# Syntax

The authority for this page is `gust-lang/src/grammar.pest`. If a form is not
listed here, it does not parse.

Gust looks like Rust and is a much smaller language. The most useful thing this
page can tell you is what is missing, so that comes first.

## What you cannot write {#what-you-cannot-write}

Every row below is valid Rust and a parse error in Gust. Each was checked
against the compiler.

| Not available | Instead |
|---|---|
| `for` / `while` / loops of any kind | Model iteration as a self-transition (`A -> A`) carrying an index, or push it into an effect |
| Method calls — `items.len()`, `s.trim()` | Declare an effect: `effect len(items: Vec<T>) -> i64` |
| Struct literals — `Order { id: x }` | Build the value in an effect and return it |
| Indexing — `items[0]` | `effect get(items: Vec<T>, index: i64) -> T` |
| `&` / `&mut` / dereferencing | Values are passed by value; the backend decides ownership |
| Closures | Not expressible; use an effect |
| `as` casts and `?` | Not expressible; use an effect |
| `"he said \"hi\""` — escaped quotes | A `"` ends the literal. String literals have no escape sequences at all |
| `/* block comment */` | Line comments only: `// like this` |
| `.5` or `3.` — floats missing a side | Write digits on both sides: `3.14` |
| `a < b < c` — chained comparison | One comparison per expression |
| `effect log(msg: String)` with no return type | The return type is mandatory: `-> ()` when there is no result |
| `Ok(v) => v,` — expression match arms | Arms take blocks: `Ok(v) => { ... }` |
| `,` between match arms | Arms are not comma-separated |
| Literal patterns — `0 => { }` | Patterns bind plain identifiers or `_` only |
| Nested patterns — `Ok(Some(x))` | One level: `Ok(x)`, `Enum::Variant(a, b)`, `_` |
| Named enum payloads — `Variant { a: i64 }` | Payloads are positional: `Variant(i64, String)` |
| `channel Events: Msg;` — trailing semicolon | Channel declarations take no semicolon |
| `send C(a, b)` — multiple send arguments | `send` carries exactly one value |
| `return` inside a handler | Rejected by the validator; use `goto` |
| A handler return type — `on go() -> i64` | Rejected by the validator; handlers return nothing |

The last two rows parse but fail `gust check` with a hard error. Everything
above them fails in the parser.

## File structure {#file-structure}

A file is any number of these items, in any order:

```
use std::EngineFailure;      // import — semicolon REQUIRED
type Order { ... }           // struct-like type
enum Tier { ... }            // enum
channel Events: Order        // channel — NO semicolon
machine Checkout { ... }     // machine
```

A `use` path is `ident ("::" ident)*`. The `std::` prefix is a Gust-virtual
namespace for [stdlib](../appendix/stdlib_api.md) machines and types: it emits no
Rust `use` and no Go import, because the consuming build is expected to compile
those sources into the same module or package.

## Machines

```
machine Name { ... }
machine Name<T: Clone + Debug> { ... }
machine Name(sends Events, receives Commands, supervises Worker(one_for_one)) { ... }
```

A machine body holds `state`, `transition`, `on`, `effect`, and `action` items
in any order. A machine with no handlers is valid.

Machine annotations combine in a single parenthesized list. See
[Channels](channels.md) for `sends` / `receives` and
[Supervision](supervision.md) for `supervises`.

## States and transitions

```
state Idle
state Running(step: String, remaining: i64)

transition start: Idle -> Running
transition finish: Running -> Done | Failed
transition run: Idle -> Done timeout 5s
```

Timeout units are `ms`, `s`, `m`, `h`. Full detail on both forms is in
[States and Transitions](states_transitions.md).

## Effects and actions

```
effect charge(order: Order) -> String
effect log(msg: String) -> ()
async effect deploy(name: String) -> String
action notify(to: String, body: String) -> String
```

The return type is mandatory. `effect` and `action` share one syntactic shape;
the keyword records replay intent. See
[Effects and Handlers](effects_handlers.md).

## Handlers

```
on pay(ctx) { ... }
on start(ctx, first_step: String) { ... }
on tick() { ... }
async on finish(ctx) { ... }
```

The handler name must match a declared transition name. The grammar also accepts
a return type (`on compute() -> i64`), but the validator rejects it — handler
return types are not supported.

Which parameter becomes the `ctx` accessor is decided by its *type*, not its
name. That rule is the single most common source of broken generated code and is
documented in full under
[the `ctx` parameter](effects_handlers.md#the-ctx-parameter).

## Statements

The complete list. There is no loop construct.

```
let total = perform charge(order);      // type annotation optional: let x: i64 = ...
goto Paid(order, receipt);              // parens optional with no args: goto Done;
perform log("message");                 // statement form
send Events(order);                     // exactly ONE argument
spawn Worker(config, 0);                // zero or more arguments
if cond { ... } else if other { ... } else { ... }
match expr { ... }
some_call(a, b);                        // bare expression statement
return value;                           // parses, but rejected in handlers
```

Since handler bodies are the only blocks in the language, `return` is
effectively unavailable: `gust check` reports
`return statements are not supported in handlers; use goto to transition`.

### `match`

Arms take **blocks**, and there are **no commas** between arms:

```gust
machine Gateway {
    state Ready
    state Done(result: String)
    state Failed(reason: String)

    transition call: Ready -> Done | Failed

    effect invoke() -> Result<String, String>

    on call() {
        let outcome = perform invoke();
        match outcome {
            Ok(msg) => {
                goto Done(msg);
            }
            Err(err) => {
                goto Failed(err);
            }
        }
    }
}
```

Patterns are exactly four shapes: `_`, `Variant`, `Variant(a, b)`, and
`Enum::Variant(a)`. Bindings are plain identifiers. Literal patterns and nested
patterns do not parse.

## Expressions

Available: literals, identifiers, nested field access, function calls,
`perform`, qualified paths, arithmetic, comparison, logic, and parentheses.

```
ctx.config.service_name          // nested field access
perform len(steps)               // perform is an expression, not only a statement
Tier::Fast                       // qualified path
helper(a, b)                     // plain function call
index + 1
a >= b && !done
```

Operators, loosest binding first:

```
1.  ||
2.  &&
3.  ==  !=  <=  >=  <  >
4.  +   -
5.  *   /   %
6.  unary !  -
```

Comparison is non-associative: the grammar allows at most one comparison
operator per expression, so `a < b < c` is a parse error.

Because `perform` is an expression, effect results compose inline:

```gust
machine Walker {
    state Executing(steps: Vec<String>, index: i64, done: Vec<String>)
    state Finished(done: Vec<String>)

    transition advance: Executing -> Executing | Finished

    effect len(steps: Vec<String>) -> i64
    effect push(done: Vec<String>, step: String) -> Vec<String>
    effect get(steps: Vec<String>, index: i64) -> String

    on advance(ctx) {
        if ctx.index >= perform len(ctx.steps) {
            goto Finished(ctx.done);
        } else {
            let step = perform get(ctx.steps, ctx.index);
            goto Executing(ctx.steps, ctx.index + 1, perform push(ctx.done, step));
        }
    }
}
```

The `else` is optional here: `goto` assigns the state and returns, so the
trailing branch is only reached when the condition is false. See
[`goto` ends the handler](states_transitions.md#goto-does-not-return).

## Generics

Machines may declare type parameters; states, effects, and handlers use them.

```
machine Saga<S> { ... }
machine Cache<T: Clone + Debug> { ... }     // bounds joined with +
```

Effects cannot declare their own type parameters, but they may use the machine's.
The validator treats a generic parameter as compatible with any type, so type
errors involving them are not reported. See [Types](types.md#generics).

## Literals and comments

```
"text"    42    3.14    true    false
// line comment — there is no block comment form
```

Two constraints follow from the grammar:

- A string literal is `"` followed by everything up to the next `"`. There are no
  escape sequences, so a literal cannot contain a quote character.
- A float needs digits on both sides of the point. `3.14` parses; `3.` and `.5`
  do not.

Identifiers start with a letter or `_`, then letters, digits, or `_`.

## Formatting

`gust fmt <file>` rewrites a file in place using the canonical layout: four-space
indentation, one item per line, and a blank line between machine sections. It
preserves comments. Run it before committing rather than hand-aligning.

## Next

- [Types](types.md) — what a type expression may contain and what it becomes
- [States and Transitions](states_transitions.md) — the state space and `goto`
- [Errors](errors.md) — what the validator checks on top of the grammar
