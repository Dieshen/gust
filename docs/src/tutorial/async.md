# Async

This page is part of the v0.1.0 documentation set for Gust.

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
