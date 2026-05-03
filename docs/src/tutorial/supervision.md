# Supervision

This page is part of the Gust documentation set.

```gust
machine DocsExample {
    state Start
    state End

    transition go: Start -> End

    on go() {
        goto End();
    }
}
```
