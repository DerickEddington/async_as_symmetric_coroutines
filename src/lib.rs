#![cfg_attr(not(windows), doc = include_str!("../README.md"))]
#![cfg_attr(windows, doc = include_str!("..\\README.md"))]
// Warn about this one but avoid annoying hits for dev-dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

use {
    core::{
        fmt::{
            self,
            Debug,
            Display,
        },
        future::Future,
    },
    flume::{
        Receiver,
        Sender,
    },
    std::error::Error,
};


/// Handle for resuming an associated `Future` that resumes an `async` coroutine.
#[derive(Debug)]
pub struct Coroutine<Input>(Sender<Input>);

impl<Input> Clone for Coroutine<Input>
{
    #[inline]
    fn clone(&self) -> Self
    {
        Self(self.0.clone())
    }
}

impl<Input> PartialEq for Coroutine<Input>
{
    #[inline]
    fn eq(
        &self,
        other: &Self,
    ) -> bool
    {
        self.0.same_channel(&other.0)
    }
}

impl<Input> Eq for Coroutine<Input> {}

impl<Input> Coroutine<Input>
{
    fn prepare() -> (Self, Self, Suspender<Input>)
    {
        let (sender, receiver) = flume::bounded(0);
        let self_handle = Self(sender);
        let myself = self_handle.clone();
        let suspender = Suspender(receiver);
        (self_handle, myself, suspender)
    }

    /// Create a new coroutine that doesn't take an initial input.  It will start executing as
    /// soon as its `Future` is first polled.
    #[inline]
    pub fn new<F: Future>(
        start: impl FnOnce(Self, Suspender<Input>) -> F
    ) -> (Self, impl Future<Output = F::Output>)
    {
        let (self_handle, myself, suspender) = Self::prepare();
        let fut = start(myself, suspender);
        (self_handle, fut)
    }

    /// Create a new coroutine that takes an initial input.  It won't execute until it's
    /// resumed/yielded-to, which supplies the initial input.
    #[inline]
    pub fn with_input<F: Future>(
        start: impl FnOnce(Self, Suspender<Input>, Input) -> F
    ) -> (Self, impl Future<Output = Result<F::Output, WithInputError>>)
    {
        let (starter_handle, myself, suspender) = Self::prepare();
        let fut = async move {
            let input = suspender.0.recv_async().await?;
            let body = start(myself, suspender, input);
            Ok(body.await)
        };
        (starter_handle, fut)
    }

    /// Resume execution of the coroutine, corresponding to `self`, from its most-recent
    /// `yield_to` suspension.  That `yield_to` call will return the given `input`.
    ///
    /// # Errors
    /// If the coroutine is not resumable.  E.g. because it finished and/or was dropped.
    #[inline]
    pub async fn resume(
        &self,
        input: Input,
    ) -> Result<(), ResumeError<Input>>
    {
        self.0.send_async(input).await.map_err(ResumeError::from)
    }
}


/// Handle for suspending an associated `Future` that suspends an `async` coroutine.
#[derive(Debug)]
pub struct Suspender<ResumeValue>(Receiver<ResumeValue>); // Must not be `Clone`.

impl<ResumeValue> PartialEq for Suspender<ResumeValue>
{
    #[inline]
    fn eq(
        &self,
        other: &Self,
    ) -> bool
    {
        self.0.same_channel(&other.0)
    }
}

impl<ResumeValue> Eq for Suspender<ResumeValue> {}

impl<ResumeValue> Suspender<ResumeValue>
{
    /// Like [`Self::try_yield_to`] but assumes those errors won't happen.
    ///
    /// It's often safe to assume those errors can't happen, depending on the logic of your
    /// coroutines, in which case this is more convenient and more readable.
    ///
    /// # Panics
    /// If any of those errors do happen.
    #[inline]
    pub async fn yield_to<Yield>(
        &self,
        other: &Coroutine<Yield>,
        value: Yield,
    ) -> ResumeValue
    {
        #![allow(clippy::panic, clippy::enum_glob_use)]

        use TryYieldError::*;

        match self.try_yield_to(other, value).await {
            Ok(resume_value) => resume_value,
            Err(OtherUnresumable { .. }) => panic!("other should still exist"),
            Err(Canceled) => panic!("handle should still exist"),
        }
    }

    /// Yield the coroutine, corresponding to `self`, suspending it, and resume the `other`
    /// coroutine with the given `value`.
    ///
    /// # Errors
    /// If either: the `other` coroutine is not resumable; or nothing can resume our coroutine
    /// anymore.
    #[inline]
    pub async fn try_yield_to<Yield>(
        &self,
        other: &Coroutine<Yield>,
        value: Yield,
    ) -> Result<ResumeValue, TryYieldError<Yield>>
    {
        other.resume(value).await?;
        self.0.recv_async().await.map_err(TryYieldError::from)
    }
}


/// Error returned by [`Coroutine::with_input`].
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(clippy::exhaustive_enums)]
pub enum WithInputError
{
    /// Our `self` [`Coroutine`] was dropped before an initial `Input` value was given.  This
    /// means our coroutine can never be started, so it's effectively canceled.
    Canceled,
}

impl From<flume::RecvError> for WithInputError
{
    #[inline]
    fn from(flume::RecvError::Disconnected: flume::RecvError) -> Self
    {
        Self::Canceled
    }
}

impl Display for WithInputError
{
    #[inline]
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result
    {
        write!(f, "associated `Coroutine` handle was dropped")
    }
}

impl Error for WithInputError {}


/// Error returned by [`Coroutine::resume`].
#[derive(Debug, Eq, PartialEq)]
#[allow(clippy::exhaustive_enums)]
pub enum ResumeError<T>
{
    /// The [`Suspender`], corresponding to `self`, was dropped before the `input` argument could
    /// be given to it.  This means the `self` coroutine can never be resumed again (but it may
    /// still finish).
    Unresumable
    {
        /// The `input` argument that couldn't be given.
        input: T,
    },
}

impl<T> From<flume::SendError<T>> for ResumeError<T>
{
    #[inline]
    fn from(value: flume::SendError<T>) -> Self
    {
        Self::Unresumable { input: value.into_inner() }
    }
}

impl<T> Display for ResumeError<T>
{
    #[inline]
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result
    {
        write!(f, "associated `Suspender` handle was dropped")
    }
}

impl<T: Debug> Error for ResumeError<T> {}


/// Error returned by [`Suspender::try_yield_to`].
#[derive(Debug, Eq, PartialEq)]
#[allow(clippy::exhaustive_enums)]
pub enum TryYieldError<T>
{
    /// The other [`Suspender`], corresponding to the `other` [`Coroutine`] argument, was dropped
    /// before the `value` argument could be yielded to it.  This means the `other` coroutine can
    /// never be resumed again (but it may still finish).
    OtherUnresumable
    {
        /// The `value` argument that couldn't be yielded.
        value: T,
    },
    /// Our [`Coroutine`], corresponding to the `self` [`Suspender`], was dropped before a
    /// `ResumeValue` was given.  This means our coroutine can never be resumed again but it is
    /// suspended at a yield point, so it's effectively canceled.
    Canceled,
}

impl<T> From<ResumeError<T>> for TryYieldError<T>
{
    #[inline]
    fn from(ResumeError::Unresumable { input: value }: ResumeError<T>) -> Self
    {
        Self::OtherUnresumable { value }
    }
}

impl<T> From<flume::RecvError> for TryYieldError<T>
{
    #[inline]
    fn from(flume::RecvError::Disconnected: flume::RecvError) -> Self
    {
        Self::Canceled
    }
}

impl<T> Display for TryYieldError<T>
{
    #[inline]
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result
    {
        write!(f, "{}", match self {
            TryYieldError::OtherUnresumable { .. } =>
                "other coroutine's `Suspender` handle was dropped",
            TryYieldError::Canceled => "associated `Coroutine` handle was dropped",
        })
    }
}

impl<T: Debug> Error for TryYieldError<T> {}
