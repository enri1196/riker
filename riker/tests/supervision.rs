#[macro_use]
extern crate riker_testkit;

use riker::actors::*;

use riker_testkit::probe::channel::{probe, ChannelProbe};
use riker_testkit::probe::{Probe, ProbeReceive};

#[derive(Clone, Debug)]
pub struct Panic;

#[derive(Clone, Debug)]
pub struct TestProbe(ChannelProbe<(), ()>);

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

#[actor(TestProbe, Panic)]
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
impl Receive<TestProbe> for PanicActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        msg.0.event(());
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
#[actor(TestProbe, Panic)]
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
impl Receive<TestProbe> for RestartSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        send_out: Option<BasicActorRef>,
    ) {
        self.actor_to_fail
            .as_ref()
            .unwrap()
            .tell(msg, send_out)
            .await;
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

#[tokio::test]
async fn supervision_restart_failed_actor() {
    let sys = ActorSystem::new().await.unwrap();

    for i in 0..100 {
        let sup = sys
            .actor_of::<RestartSup>(&format!("supervisor_{}", i))
            .await
            .unwrap();

        // Make the test actor panic
        sup.tell(Panic, None).await;

        let (probe, mut listen) = probe::<()>();
        sup.tell(TestProbe(probe), None).await;
        p_assert_eq!(listen, ());
    }
}

// Test Escalate Strategy
#[actor(TestProbe, Panic)]
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
impl Receive<TestProbe> for EscalateSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        send_out: Option<BasicActorRef>,
    ) {
        self.actor_to_fail
            .as_ref()
            .unwrap()
            .tell(msg, send_out)
            .await;
    }
}

#[async_trait::async_trait]
impl Receive<Panic> for EscalateSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        send_out: Option<BasicActorRef>,
    ) {
        self.actor_to_fail
            .as_ref()
            .unwrap()
            .tell(Panic, send_out)
            .await;
    }
}

#[actor(TestProbe, Panic)]
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

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Restart
    }
}

#[async_trait::async_trait]
impl Receive<TestProbe> for EscRestartSup {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        send_out: Option<BasicActorRef>,
    ) {
        self.escalator.as_ref().unwrap().tell(msg, send_out).await;
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

#[tokio::test]
async fn supervision_escalate_failed_actor() {
    let sys = ActorSystem::new().await.unwrap();

    let sup = sys.actor_of::<EscRestartSup>("supervisor").await.unwrap();

    // Make the test actor panic
    sup.tell(Panic, None).await;

    let (probe, mut listen) = probe::<()>();
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    sup.tell(TestProbe(probe), None).await;
    p_assert_eq!(listen, ());
    sys.print_tree();
}
