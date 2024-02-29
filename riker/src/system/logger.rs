use std::sync::Arc;

use tracing::info;

use crate::actor::{
    Actor, ActorFactoryArgs, ActorRef, All, BasicActorRef, ChannelMsg, Context, DeadLetter,
    Subscribe, Tell,
};

use super::actor_ref::BoxedTell;

/// Simple actor that subscribes to the dead letters channel and logs using the default logger
pub struct DeadLetterLogger {
    dl_chan: ActorRef<ChannelMsg<DeadLetter>>,
}

impl ActorFactoryArgs<ActorRef<ChannelMsg<DeadLetter>>> for DeadLetterLogger {
    fn create_args(dl_chan: ActorRef<ChannelMsg<DeadLetter>>) -> Self {
        DeadLetterLogger { dl_chan }
    }
}

#[async_trait::async_trait]
impl Actor for DeadLetterLogger {
    type Msg = DeadLetter;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        self.dl_chan
            .tell(
                Subscribe {
                    topic: All.into(),
                    actor: sub,
                },
                None,
            )
            .await;
    }

    async fn recv(&mut self, _: &Context<Self::Msg>, msg: Self::Msg, _: Option<BasicActorRef>) {
        info!(
            "DeadLetter: {:?} => {:?} ({:?})",
            msg.send_out, msg.recipient, msg.msg
        )
    }
}
