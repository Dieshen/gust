# Syntax

This reference summarizes the most important Gust syntax forms.

## Machine and states

```gust
machine Payment {
    state Pending(amount: i64)
    state Settled(amount: i64)

    transition settle: Pending -> Settled

    on settle() {
        goto Settled(amount);
    }
}
```

## Generics

```gust
machine Cache<T: Clone> {
    state Empty
    state Full(value: T)

    transition put: Empty -> Full

    on put(value: T) {
        goto Full(value);
    }
}
```

## Effects and match

```gust
machine Gateway {
    state Ready
    state Done(result: String)
    state Failed(reason: String)

    transition call: Ready -> Done | Failed

    async effect invoke() -> Result<String, String>

    async on call() {
        let result = perform invoke();
        match result {
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

## Concurrency primitives

- `channel <Name>: <Type> (capacity: N, mode: broadcast|mpsc)`
- machine annotations: `sends`, `receives`, `supervises`
- `send Channel(expr);`
- `spawn ChildMachine(...);`
- transition timeout: `transition run: A -> B timeout 5s`
