# Gust syntax reference

The complete surface of the language, derived from `gust-lang/src/grammar.pest`. If a form is not here, it does not parse.

## Contents

- [Top level](#top-level)
- [Types](#types)
- [Machines](#machines)
- [States](#states)
- [Transitions and timeouts](#transitions-and-timeouts)
- [Effects and actions](#effects-and-actions)
- [Handlers](#handlers)
- [Statements](#statements)
- [Expressions](#expressions)
- [Generics](#generics)
- [Channels](#channels)
- [Supervision](#supervision)
- [Comments and literals](#comments-and-literals)

## Top level

A file is any number of these, in any order:

```gust
use std::EngineFailure;      // import — semicolon required
type Order { ... }           // struct-like type
enum Tier { ... }            // enum
channel Events: Order        // channel — NO semicolon
machine Checkout { ... }     // machine
```

## Types

Structs use `type`, not `struct`:

```gust
type Order {
    id: String,
    total: i64,
    tags: Vec<String>,
}
```

Enum payloads are **positional only** — named payload fields do not parse:

```gust
enum Tier { Fast, Slow }                        // fieldless
enum Failure { Timeout(i64), Rejected(String) } // positional payloads
// enum Bad { Variant { code: i64 } }           // does NOT parse
```

Type expressions: `String`, `i64`, `bool`, `Vec<T>`, `Result<T, E>`, `HashMap<K, V>`, `()`, tuples `(A, B)`, and any identifier. Type names you don't declare are passed through to the host language — which is how the `ctx` placeholder types work, and also means a typo in a type name is not caught here.

## Machines

```gust
machine Name { ... }
machine Name<T: Clone> { ... }
machine Name(sends Events, supervises Worker(one_for_one)) { ... }
```

A machine body holds `state`, `transition`, `on`, `effect`, and `action` items in any order. A machine with no handlers is valid — useful for a supervisor that only defines a state space.

## States

```gust
state Idle
state Running(step: String, remaining: i64)
```

Fields are in scope inside handlers whose transition starts from that state — either bare (`step`) or via the ctx parameter (`ctx.step`). See the ctx section of SKILL.md.

## Transitions and timeouts

```gust
transition start: Idle -> Running
transition finish: Running -> Done | Failed        // multiple targets
transition run: Idle -> Done timeout 5s
```

Timeout units: `ms`, `s`, `m`, `h`. Note the semantics are narrow: `timeout` bounds how long the **handler** may run (it wraps the body in `tokio::time::timeout`) and on expiry returns `Err(...Failed { reason: "…timed out after…" })` without changing state. It is not a clock on how long the machine may sit in a state — see the timeouts section of `patterns.md` for modeling elapsed time.

`goto` arguments are zipped **positionally** with the target state's declared fields, so order matters and arity is checked:

```gust
state Running(step: String, remaining: i64)
goto Running(next_step, next_remaining);   // step <- next_step, remaining <- next_remaining
```

## Effects and actions

The return type is **mandatory** — use `-> ()` when there is no result:

```gust
effect charge(order: Order) -> String
effect log(msg: String) -> ()
async effect deploy(name: String) -> String
action notify(to: String, body: String) -> String
```

`effect` is assumed replay-safe; `action` is not (see SKILL.md). Both are invoked with `perform`. Effects cannot declare their own generic parameters, but they may use the machine's.

Each machine with effects generates a `{Machine}Effects` trait in the target language, with one method per effect.

## Handlers

```gust
on pay(ctx) { ... }
on start(ctx, first_step: String) { ... }
on tick() { ... }
async on finish(ctx) { ... }
on compute(ctx) -> i64 { ... }        // return type optional
```

The handler name must match a declared transition name. Every path through the body should end in `goto` — a `goto` acts like a return, so an early `goto` inside an `if` with no `else` is a normal pattern:

```gust
on execute_next() {
    if index >= perform len(steps) {
        goto Committed(completed);
    }
    let current = perform get_step(steps, index);
    goto Executing(steps, index + 1, completed);
}
```

## Statements

The complete list — there is no loop construct:

```gust
let total = perform charge(order);          // type annotation optional: let x: i64 = ...
return value;
goto Paid(order, receipt);                  // parens optional when no args: goto Done;
perform log("message");                     // as a statement
send Events(order);                         // exactly ONE argument
spawn Worker(config, 0);                    // zero or more arguments
if cond { ... } else if other { ... } else { ... }
match expr { ... }
some_call(a, b);                            // bare expression statement
```

### match

Arms take **blocks**, and there are **no commas** between arms:

```gust
match result {
    Ok(msg) => {
        goto Done(msg);
    }
    Err(err) => {
        goto Failed(err);
    }
    _ => {
        goto Failed("unknown");
    }
}
```

Patterns are only: `_`, `Variant`, `Variant(a, b)`, or `Enum::Variant(a)`. Bindings are plain identifiers. Literal patterns (`0 =>`) and nested patterns (`Ok(Some(x))`) do not parse.

## Expressions

Available: literals, identifiers, nested field access, function calls, `perform`, qualified paths, arithmetic, comparison, logic, parentheses.

```gust
ctx.config.service_name          // nested field access is fine
perform len(steps)               // perform is an expression
Tier::Fast                       // qualified path
helper(a, b)                     // plain function call
index + 1
a >= b && !done
```

Operators by precedence, loosest first: `||`, `&&`, comparison (`== != <= >= < >`), `+ -`, `* / %`, unary (`! -`).

Only **one comparison per expression** — `a < b < c` does not parse.

**Not expressible:** method calls (`x.len()`), struct literals (`Order { .. }`), indexing (`v[0]`), references (`&x`), closures, ranges, `as` casts, `?`. Route all of these through effects.

## Generics

Machines may be generic; effects and states use the machine's parameters:

```gust
machine Saga<S> {
    state Planning(steps: Vec<S>)
    effect get_step(steps: Vec<S>, index: i64) -> S
}

machine Cache<T: Clone + Debug> { ... }     // bounds joined with +
```

The validator treats generic parameters as compatible with any type, so type errors involving them are not caught at the Gust level.

## Channels

Declared at top level, **no trailing semicolon**:

```gust
type Order { id: String }

channel Orders: Order
channel Events: Order (capacity: 64, mode: broadcast)
```

The payload type must be declared — since 1.0 an unknown type name is a
validator error rather than a name passed through to the host language.

Modes: `broadcast`, `mpsc`. Machines declare their relationship to a channel via annotations, and `send` pushes one value:

```gust
machine Producer(sends Orders) {
    on emit(ctx) {
        send Orders(ctx.order);
        goto Idle;
    }
}

machine Consumer(receives Orders) { ... }
```

## Supervision

```gust
machine Engine(supervises Worker(one_for_one)) { ... }
```

Strategies: `one_for_one` (restart only the failed child), `one_for_all` (restart all children), `rest_for_one` (restart the failed child and those started after it). Children are started with `spawn`:

```gust
spawn Worker(config);
```

Multiple annotations combine in one parenthesized list:

```gust
machine Coordinator(sends Events, receives Commands, supervises Worker(one_for_all)) { ... }
```

## Comments and literals

```gust
// Line comments only — there is no block comment form.
```

Literals: `"text"`, `42`, `3.14`, `true`, `false`.

Two constraints:
- **Strings have no escape sequences.** A `"` ends the literal, so escaped quotes are not possible.
- **Floats need digits on both sides** of the point: `3.14` parses, `3.` and `.5` do not.

Identifiers start with a letter or `_`, then letters, digits, or `_`.
