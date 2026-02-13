# no_std Example

## Build

```bash
gust build --target nostd sensor.gu
```

## Notes

- Generated output is `#![no_std]`.
- Dynamic containers map to `heapless` where possible.
- This scaffold is intended for embedded integration.
