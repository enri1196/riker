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

#[actor(Panic)]
#[derive(Default)]
struct EscalateSup {
    actor_to_fail: Option<ActorRef<PanicActorMsg>>,
}

#[async_trait::async_trait]
impl Actor for EscalateSup {
    type Msg = EscalateSupMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.actor_to_fail = ctx.actor_of::<PanicActor>("actor-to-fail").await.ok();
    }

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Escalate
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
        // match msg {
        //     // We just resend the messages to the actor that we're concerned about testing
        //     TestMsg::Panic => self.actor_to_fail.try_tell(msg, None).unwrap(),
        //     TestMsg::Probe(_) => self.actor_to_fail.try_tell(msg, None).unwrap(),
        // };
    }
}

#[async_trait::async_trait]
impl Receive<Panic> for EscalateSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        _send_out: Option<BasicActorRef>,
    ) {
        self.actor_to_fail.as_ref().unwrap().tell(Panic, None).await;
    }
}

#[actor(Panic)]
#[derive(Default)]
struct EscRestartSup {
    escalator: Option<ActorRef<EscalateSupMsg>>,
}

#[async_trait::async_trait]
impl Actor for EscRestartSup {
    type Msg = EscRestartSupMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.escalator = ctx
            .actor_of::<EscalateSup>("escalate-supervisor")
            .await
            .ok();
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
        self.receive(ctx, msg, send_out).await;
        // match msg {
        //     // We resend the messages to the parent of the actor that is/has panicked
        //     TestMsg::Panic => self.escalator.try_tell(msg, None).unwrap(),
        //     TestMsg::Probe(_) => self.escalator.try_tell(msg, None).unwrap(),
        // };
    }
}

#[async_trait::async_trait]
impl Receive<Panic> for EscRestartSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        _send_out: Option<BasicActorRef>,
    ) {
        self.escalator.as_ref().unwrap().tell(Panic, None).await;
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let sup = sys.actor_of::<EscRestartSup>("supervisor").await.unwrap();

    println!("Before panic we see supervisor and actor that will panic!");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();

    sup.tell(Panic, None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("We should see panic printed, but we still alive and panic actor still here!");
    sys.print_tree();
}
