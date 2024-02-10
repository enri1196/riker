#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use futures::FutureExt;
use tokio::sync::oneshot::{channel, Sender as ChannelSender};
use tokio::task::JoinHandle;
use crate::actors::*;

/// Convenience fuction to send and receive a message from an actor
///
/// This function sends a message `msg` to the provided actor `receiver`
/// and returns a `Future` which will be completed when `receiver` replies
/// by sending a message to the `sender`. The sender is a temporary actor
/// that fulfills the `Future` upon receiving the reply.
///
/// `futures::future::RemoteHandle` is the future returned and the task
/// is executed on the provided executor `ctx`.
///
/// This pattern is especially useful for interacting with actors from outside
/// of the actor system, such as sending data from HTTP request to an actor
/// and returning a future to the HTTP response, or using await.
///
/// # Examples
///
/// ```
/// # use riker::actors::*;
/// # use riker::ask::ask;
/// # use tokio::task::JoinHandle;
///
/// #[derive(Default)]
/// struct Reply;
///
/// impl Actor for Reply {
///    type Msg = String;
///
///    fn recv(&mut self,
///                 ctx: &Context<Self::Msg>,
///                 msg: Self::Msg,
///                 sender: Sender) {
///         // reply to the temporary ask actor
///         sender.as_ref().unwrap().try_tell(
///             format!("Hello {}", msg), None
///         ).unwrap();
///     }
/// }
///
/// // set up the actor system
/// # tokio_test::block_on(async {
/// let sys = ActorSystem::new().unwrap();
///
/// // create instance of Reply actor
/// let actor = sys.actor_of::<Reply>("reply").unwrap();
///
/// // ask the actor
/// let msg = "Will Riker".to_string();
/// let r: JoinHandle<String> = ask(&sys, &actor, msg);
///
/// assert_eq!(r.await.unwrap(), "Hello Will Riker".to_string());
/// # })
/// ```

pub trait Ask<Msg: Message> {
    fn ask<Ret: Message>(&self, msg: Msg) -> JoinHandle<Ret>;
}

impl<Msg: Message> Ask<Msg> for ActorRef<Msg> {
    fn ask<Ret>(&self, msg: Msg) -> JoinHandle<Ret>
    where
        Ret: Message,
    {
        let (tx, rx) = channel::<Ret>();
        let tx = Arc::new(Mutex::new(Some(tx)));
        let sys = self.cell.system();

        let props = Props::new_from_args(Box::new(AskActor::new), tx);
        let actor = sys.tmp_actor_of_props(props).unwrap();
        self.tell(msg, Some(actor.into()));

        sys.run(rx.map(|r| r.unwrap()))
    }
}

impl<Msg: Message> Ask<Msg> for BasicActorRef {
    fn ask<Ret: Message>(&self, msg: Msg) -> JoinHandle<Ret> {
        let (tx, rx) = channel::<Ret>();
        let tx = Arc::new(Mutex::new(Some(tx)));
        let sys = self.cell.system();

        let props = Props::new_from_args(Box::new(AskActor::new), tx);
        let actor = sys.tmp_actor_of_props(props).unwrap();
        self.try_tell(msg, Some(actor.into())).unwrap();

        sys.run(rx.map(|r| r.unwrap()))
    }
}

struct AskActor<Msg> {
    tx: Arc<Mutex<Option<ChannelSender<Msg>>>>,
}

impl<Msg: Message> AskActor<Msg> {
    fn new(tx: Arc<Mutex<Option<ChannelSender<Msg>>>>) -> BoxActor<Msg> {
        let ask = AskActor { tx };
        Box::new(ask)
    }
}

impl<Msg: Message> Actor for AskActor<Msg> {
    type Msg = Msg;

    fn recv(&mut self, ctx: &Context<Msg>, msg: Msg, _: Sender) {
        if let Ok(mut tx) = self.tx.lock() {
            tx.take().unwrap().send(msg).unwrap();
        }
        ctx.stop(ctx.myself());
    }
}
