extern crate riker;
use riker::actors::*;

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Panic;

#[derive(Default)]
struct DumbActor;

#[async_trait::async_trait]
impl Actor for DumbActor {
    type Msg = ();

    async fn recv(
        &mut self,
        _: &Context<Self::Msg>,
        _: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
    }
}

#[actor(Panic)]
#[derive(Default)]
struct PanicActor;

#[async_trait::async_trait]
impl Actor for PanicActor {
    type Msg = PanicActorMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        ctx.actor_of::<DumbActor>("child_a").await.unwrap();

        ctx.actor_of::<DumbActor>("child_b").await.unwrap();

        ctx.actor_of::<DumbActor>("child_c").await.unwrap();

        ctx.actor_of::<DumbActor>("child_d").await.unwrap();
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
impl Receive<Panic> for PanicActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        _send_out: Option<BasicActorRef>,
    ) {
        panic!("// TEST PANIC // TEST PANIC // TEST PANIC //");
    }
}

// Test Restart Strategy
#[actor(Panic)]
#[derive(Default)]
struct RestartSup {
    actor_to_fail: Option<ActorRef<PanicActorMsg>>,
}

#[async_trait::async_trait]
impl Actor for RestartSup {
    type Msg = RestartSupMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.actor_to_fail = ctx.actor_of::<PanicActor>("actor-to-fail").await.ok();
    }

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Restart
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await
    }
}

#[async_trait::async_trait]
impl Receive<Panic> for RestartSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        _send_out: Option<BasicActorRef>,
    ) {
        self.actor_to_fail.as_ref().unwrap().tell(Panic, None).await;
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let sup = sys.actor_of::<RestartSup>("supervisor").await.unwrap();
    // println!("Child not added yet");
    // sys.print_tree();

    println!("Before panic we see supervisor and actor that will panic!");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();

    sup.tell(Panic, None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("We should see panic printed, but we still alive and panic actor still here!");
    sys.print_tree();
}
