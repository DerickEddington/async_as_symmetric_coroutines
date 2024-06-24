#![cfg(test)] // Satisfy the `clippy::tests_outside_test_module` lint.
#![cfg_attr(test, allow(unused_crate_dependencies))]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::incompatible_msrv,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::use_debug
)]

use {
    async_as_symmetric_coroutines::{
        Coroutine,
        Suspender,
    },
    futures_util::join,
    pollster::block_on,
    std::{
        sync::OnceLock,
        thread,
    },
};


static A: OnceLock<Coroutine<u8>> = OnceLock::new();
static B: OnceLock<Coroutine<u16>> = OnceLock::new();
static C: OnceLock<Coroutine<u32>> = OnceLock::new();
static D: OnceLock<Coroutine<u64>> = OnceLock::new();


async fn a(
    _myself: Coroutine<u8>,
    suspend: Suspender<u8>,
    input_1: u8,
) -> u8
{
    println!("[A] start input_1: {:?}", input_1);

    let (b, c, d) = (get_b(), get_c(), get_d());

    let input_2 = suspend.yield_to(b, (input_1 + 1).into()).await;
    println!("[A] resume input_2: {:?}", input_2);

    let input_3 = suspend.yield_to(c, (input_2 + 1).into()).await;
    println!("[A] resume input_3: {:?}", input_3);

    let input_4 = suspend.yield_to(d, (input_3 + 1).into()).await;
    println!("[A] done input_4: {:?}", input_4);

    input_4 + 1
}

fn get_a() -> &'static Coroutine<u8>
{
    A.get().unwrap()
}


async fn b(
    _myself: Coroutine<u16>,
    _suspend: Suspender<u16>,
    input: u16,
)
{
    println!("[B] start input: {:?}", input);
    get_c().resume((input * 2).into()).await.unwrap();
}

fn get_b() -> &'static Coroutine<u16>
{
    B.get().unwrap()
}


async fn c(
    _myself: Coroutine<u32>,
    suspend: Suspender<u32>,
    input_1: u32,
)
{
    println!("[C] start input_1: {:?}", input_1);
    let input_2 = suspend.yield_to(get_d(), (input_1 * 3).into()).await;
    println!("[C] resume input_2: {:?}", input_2);
    get_d().resume((input_2 * 3).into()).await.unwrap();
}

fn get_c() -> &'static Coroutine<u32>
{
    C.get().unwrap()
}


async fn d(
    _myself: Coroutine<u64>,
    suspend: Suspender<u64>,
    input_1: u64,
)
{
    #![allow(clippy::as_conversions, clippy::as_underscore, clippy::cast_possible_truncation)]

    println!("[D] start input_1: {:?}", input_1);
    let input_2 = suspend.yield_to(get_a(), input_1 as _).await;
    println!("[D] resume input_2: {:?}", input_2);
    let input_3 = suspend.yield_to(get_a(), input_2 as _).await;
    println!("[D] resume input_3: {:?}", input_3);
    get_a().resume(input_3 as _).await.unwrap();
}

fn get_d() -> &'static Coroutine<u64>
{
    D.get().unwrap()
}


#[test]
fn multi()
{
    let input = 234;

    let (a, a_fut) = Coroutine::with_input(a);
    A.set(a).unwrap();

    let (b, b_fut) = Coroutine::with_input(b);
    B.set(b).unwrap();

    let (c, c_fut) = Coroutine::with_input(c);
    C.set(c).unwrap();

    let (d, d_fut) = Coroutine::with_input(d);
    D.set(d).unwrap();


    // Drive these to completion, on multiple threads
    let t1 = thread::spawn(|| block_on(async { join!(d_fut, b_fut) }));
    let t2 = thread::spawn(|| block_on(a_fut));

    // Start the coroutines.
    block_on(get_a().resume(input)).unwrap();

    // Drive the remaining one to completion.
    block_on(c_fut).unwrap();

    {
        let (rd, rb) = t1.join().unwrap();
        rd.unwrap();
        rb.unwrap();
    }

    let ra = t2.join().unwrap();
    assert_eq!(ra, Ok(139));
}
