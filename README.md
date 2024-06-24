# `async_as_symmetric_coroutines`

Helpers to use `async` code as symmetric coroutines in stable Rust.  Just an experiment of mine
that became a bit more developed.  The general design of this might have potential when coroutines
need to move between threads (e.g. for work stealing), since that seems possible when these are
`Send`, as opposed to other coroutine designs that have a call-stack for each which can't be
`Send`.

## Example

```rust
use async_as_symmetric_coroutines::{Coroutine, Suspender};

type Input1 = Option<Coroutine<Input2>>;
type Input2 = i32;

async fn coro1(_myself: Coroutine<Input1>, suspend: Suspender<Input1>, input: Input1) {
    let coro2 = input.unwrap();
    let input = suspend.yield_to(&coro2, 123).await;
    assert!(input.is_none());
}

let (coro1, coro1_fut) = Coroutine::with_input(coro1);

let (_coro2, coro2_fut) = Coroutine::new(|myself, suspend| async move {
    let input = suspend.yield_to(&coro1, Some(myself.clone())).await;
    coro1.resume(None).await;
    input.to_string()
});

let t1 = std::thread::spawn(|| pollster::block_on(coro1_fut));
let t2 = std::thread::spawn(|| pollster::block_on(coro2_fut));
let r1 = t1.join().unwrap();
assert_eq!(r1, Ok(()));
let r2 = t2.join().unwrap();
assert_eq!(r2, "123");
```

See the `examples/` and `tests/` for more examples.
