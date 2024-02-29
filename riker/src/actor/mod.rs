pub(crate) mod actor_cell;
pub(crate) mod actor_ref;
pub(crate) mod actor_traits;
pub(crate) mod channel;
pub(crate) mod macros;
pub(crate) mod props;
pub(crate) mod selection;
pub(crate) mod uri;

use std::fmt;

use crate::validate::InvalidName;

// Public riker::actor API (plus the pub data types in this file)
pub use self::{
    actor_traits::*,
    actor_cell::Context,
    actor_ref::{ActorRef, BasicActorRef, BoxedTell},
    actor_traits::*,
    channel::{
        channel, All, Channel, ChannelMsg, ChannelRef, DLChannelMsg, DeadLetter, EventsChannel,
        Publish, Subscribe, SysTopic, Topic, Unsubscribe, UnsubscribeAll,
    },
    macros::actor,
    props::{ActorArgs, ActorFactory, ActorFactoryArgs, ActorProducer, BoxActorProd, Props},
    selection::{ActorSelection, ActorSelectionFactory, RefSelectionFactory},
    uri::{ActorPath, ActorUri},
};

use crate::system::SystemMsg;

pub type MsgResult<T> = Result<(), MsgError<T>>;

/// Internal message error when a message can't be added to an actor's mailbox
#[doc(hidden)]
#[derive(Clone)]
pub struct MsgError<T> {
    pub msg: T,
}

impl<T> MsgError<T> {
    pub fn new(msg: T) -> Self {
        MsgError { msg }
    }
}

impl<T> fmt::Display for MsgError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("The actor does not exist. It may have been terminated")
    }
}

impl<T> fmt::Debug for MsgError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

/// Error type when an `try_tell` fails on `Option<ActorRef<Msg>>`
pub struct TryMsgError<T> {
    pub msg: T,
}

impl<T> TryMsgError<T> {
    pub fn new(msg: T) -> Self {
        TryMsgError { msg }
    }
}

impl<T> fmt::Display for TryMsgError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Option<ActorRef> is None")
    }
}

impl<T> fmt::Debug for TryMsgError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

/// Error type when an actor fails to start during `actor_of`.
#[derive(Debug)]
pub enum CreateError {
    Panicked,
    System,
    InvalidName(String),
    AlreadyExists(ActorPath),
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Panicked => {
                f.write_str("Failed to create actor. Cause: Actor panicked while starting")
            }
            Self::System => f.write_str("Failed to create actor. Cause: System failure"),
            Self::InvalidName(ref name) => f.write_str(&format!(
                "Failed to create actor. Cause: Invalid actor name ({})",
                name
            )),
            Self::AlreadyExists(ref path) => f.write_str(&format!(
                "Failed to create actor. Cause: An actor at the same path already exists ({})",
                path
            )),
        }
    }
}

impl From<InvalidName> for CreateError {
    fn from(err: InvalidName) -> CreateError {
        CreateError::InvalidName(err.name)
    }
}

/// Error type when an actor fails to restart.
pub struct RestartError;

impl fmt::Display for RestartError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Failed to restart actor. Cause: Actor panicked while starting")
    }
}

impl fmt::Debug for RestartError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

#[async_trait::async_trait]
impl<A: Actor + ?Sized> Actor for Box<A> {
    type Msg = A::Msg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        (**self).pre_start(ctx).await;
    }

    async fn post_start(&mut self, ctx: &Context<Self::Msg>) {
        (**self).post_start(ctx).await
    }

    async fn post_stop(&mut self) {
        (**self).post_stop().await
    }

    async fn sys_recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemMsg,
        send_out: Option<BasicActorRef>,
    ) {
        (**self).sys_recv(ctx, msg, send_out).await
    }

    fn supervisor_strategy(&self) -> Strategy {
        (**self).supervisor_strategy()
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        (**self).recv(ctx, msg, send_out).await
    }
}

/// The actor trait object
pub type BoxActor<Msg> = Box<dyn Actor<Msg = Msg> + Send>;

/// Supervision strategy
///
/// Returned in `Actor.supervision_strategy`
pub enum Strategy {
    /// Stop the child actor
    Stop,

    /// Attempt to restart the child actor
    Restart,

    /// Escalate the failure to a parent
    Escalate,
}
