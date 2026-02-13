# Errors

This page documents Phase 4 ecosystem guidance for Gust.

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
