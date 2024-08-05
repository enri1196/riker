use std::sync::Arc;
use tokio::sync::Mutex;

use crate::actors::*;
use futures::FutureExt;
use tokio::sync::oneshot::{channel, Sender as ChannelSender};
use tokio::task::JoinHandle;

/// Convenience fuction to send and receive a message from an actor
///
/// This function sends a message `msg` to the provided actor `receiver`
/// and returns a `Future` which will be completed when `receiver` replies
/// by sending a message to the `sender`. The sender is a temporary actor
/// that fulfills the `Future` upon receiving the reply.
///
/// `tokio::task::JoinHandle` is the future returned and the task
/// is executed on the provided executor.
///
/// This pattern is especially useful for interacting with actors from outside
/// of the actor system, such as sending data from HTTP request to an actor
/// and returning a future to the HTTP response, or using await.
///
/// # Examples
///
/// ```
/// # use riker::actors::*;
/// # use riker::ask::Ask;
/// # use tokio::task::JoinHandle;
///
/// #[derive(Default)]
/// struct Reply;
///
/// #[async_trait::async_trait]
/// impl Actor for Reply {
///    type Msg = String;
///
///    async fn recv(&mut self,
///                 ctx: &Context<Self::Msg>,
///                 msg: Self::Msg,
///                 send_out: Option<BasicActorRef>) {
///         // reply to the temporary ask actor
///         send_out.as_ref().unwrap().try_tell(
///             format!("Hello {}", msg), None
///         ).await.unwrap();
///     }
/// }
///
/// // set up the actor system
/// # tokio_test::block_on(async {
/// let sys = ActorSystem::new().await.unwrap();
///
/// // create instance of Reply actor
/// let actor = sys.actor_of::<Reply>("reply").await.unwrap();
///
/// // ask the actor
/// let msg = "Will Riker".to_string();
/// let r = actor.ask::<String>(msg);
///
/// assert_eq!(r.await.unwrap(), "Hello Will Riker".to_string());
/// # })
/// ```
pub trait Ask<Msg: Message> {
    fn ask<Ret: Message>(&self, msg: Msg) -> JoinHandle<Ret>;
}

impl<Msg: Message> Ask<Msg> for ActorRef<Msg> {
    fn ask<Ret: Message>(&self, msg: Msg) -> JoinHandle<Ret> {
        let my_self = self.clone();
        let sys = self.system().clone();
        self.system().run(async move {
            let (tx, rx) = channel::<Ret>();
            let tx = Arc::new(Mutex::new(Some(tx)));
            let props = Props::new_from_args(AskActor::boxed_new, tx);
            let actor = sys.tmp_actor_of_props(props).await.unwrap();
            my_self.tell(msg, Some(actor.into())).await;
            rx.map(|r| r.unwrap()).await
        })
    }
}

impl<Msg: Message> Ask<Msg> for BasicActorRef {
    fn ask<Ret: Message>(&self, msg: Msg) -> JoinHandle<Ret> {
        let my_self = self.clone();
        let sys = self.system().clone();
        self.system().run(async move {
            let (tx, rx) = channel::<Ret>();
            let tx = Arc::new(Mutex::new(Some(tx)));
            let props = Props::new_from_args(AskActor::boxed_new, tx);
            let actor = sys.tmp_actor_of_props(props).await.unwrap();
            my_self.try_tell(msg, Some(actor.into())).await.unwrap();
            rx.map(|r| r.unwrap()).await
        })
    }
}

struct AskActor<Msg> {
    tx: Arc<Mutex<Option<ChannelSender<Msg>>>>,
}

impl<Msg: Message> AskActor<Msg> {
    fn boxed_new(tx: Arc<Mutex<Option<ChannelSender<Msg>>>>) -> BoxActor<Msg> {
        Box::new(AskActor { tx })
    }
}

#[async_trait::async_trait]
impl<Msg: Message> Actor for AskActor<Msg> {
    type Msg = Msg;

    async fn recv(&mut self, ctx: &Context<Msg>, msg: Msg, _: Option<BasicActorRef>) {
        let mut lock_tx = self.tx.lock().await;
        lock_tx.take().and_then(|tx| tx.send(msg).ok());
        ctx.stop(ctx.myself()).await;
    }
}
