//! A contrived example with print-outs that show the interleaved sequence of executions.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::incompatible_msrv,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::print_stdout,
    clippy::shadow_unrelated,
    clippy::unwrap_used,
    clippy::use_debug,
    unused_crate_dependencies
)]

use {
    async_as_symmetric_coroutines::Coroutine,
    core::cell::OnceCell,
    futures_util::join,
    pollster::block_on,
};


fn main()
{
    // Helps achieve mutual (cyclic) referencing.
    let (coroutine, main_coro) = (OnceCell::new(), OnceCell::new());
    let (get_coroutine, get_main_coro) =
        (|| coroutine.get().unwrap(), || main_coro.get().unwrap());

    println!("[main] creating `coroutine`");

    let (coroutine_handle, coroutine_fut) =
        Coroutine::with_input(|_myself, suspend, input| async move {
            println!("[coroutine] started with input {}", input);
            let main_coro = get_main_coro();
            for i in 0 .. 5 {
                println!("[coroutine] yielding {}", i);
                let input = suspend.yield_to(main_coro, Some(i)).await;
                println!("[coroutine] got {} from `main_coro`", input);
            }
            let _ = main_coro.resume(None).await; // Tell `main_coro` we're finished.
            println!("[coroutine] exiting");
        });
    coroutine.set(coroutine_handle).unwrap();

    let (main_coro_handle, main_fut) = Coroutine::new(|_myself, suspend| async move {
        println!("[main_coro] started");
        let coroutine = get_coroutine();
        let mut counter = 100_u16;
        loop {
            println!("[main_coro] resuming `coroutine` with argument {}", counter);
            match suspend.yield_to(coroutine, counter).await {
                Some(i) => println!("[main_coro] got {:?} from `coroutine`", i),
                None => break, // `coroutine` finished.
            }
            counter += 1;
        }
        println!("[main_coro] exiting");
    });
    main_coro.set(main_coro_handle).unwrap();

    // Drive them to completion.
    let (_, ()) = block_on(async { join!(coroutine_fut, main_fut) });

    println!("[main] exiting");
}


#[test]
fn basic()
{
    main()
}
