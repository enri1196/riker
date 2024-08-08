use riker::actors::*;

use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct SomeMessage;

#[actor(SomeMessage)]
#[derive(Default)]
struct DumbActor;

#[async_trait::async_trait]
impl Actor for DumbActor {
    type Msg = DumbActorMsg;

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
    }
}

#[async_trait::async_trait]
impl Receive<SomeMessage> for DumbActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SomeMessage,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: -> got msg: {:?} ", ctx.myself().name(), msg);
    }
}

// *** Publish test ***
#[actor(DeadLetter)]
#[derive(Default)]
struct DeadLetterActor;

#[async_trait::async_trait]
impl Actor for DeadLetterActor {
    type Msg = DeadLetterActorMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let topic = Topic::from("*");

        println!(
            "{}: pre_start subscribe to topic {:?}",
            ctx.myself().name(),
            topic
        );
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));

        ctx.system()
            .dead_letters()
            .tell(Subscribe { actor: sub, topic }, None)
            .await;
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
    }
}

#[async_trait::async_trait]
impl Receive<DeadLetter> for DeadLetterActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: DeadLetter,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: -> got msg: {:?} ", ctx.myself().name(), msg);
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let _sub = sys
        .actor_of::<DeadLetterActor>("system-actor")
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Creating dumb actor");
    let dumb = sys.actor_of::<DumbActor>("dumb-actor").await.unwrap();

    println!("Stopping dumb actor");
    sys.stop(&dumb).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Sending SomeMessage to stopped actor");
    dumb.tell(SomeMessage, None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
