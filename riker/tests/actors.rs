#[macro_use]
extern crate riker_testkit;

use riker::actors::*;

use riker_testkit::probe::channel::{probe, ChannelProbe};
use riker_testkit::probe::{Probe, ProbeReceive};

#[derive(Clone, Debug)]
pub struct Add;

#[derive(Clone, Debug)]
pub struct TestProbe(ChannelProbe<(), ()>);

#[actor(TestProbe, Add)]
#[derive(Default)]
struct Counter {
    probe: Option<TestProbe>,
    count: u32,
}

#[async_trait::async_trait]
impl Actor for Counter {
    // we used the #[actor] attribute so CounterMsg is the Msg type
    type Msg = CounterMsg;

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
impl Receive<TestProbe> for Counter {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe = Some(msg)
    }
}

#[async_trait::async_trait]
impl Receive<Add> for Counter {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Add,
        _send_out: Option<BasicActorRef>,
    ) {
        self.count += 1;
        if self.count == 1_000_000 {
            self.probe.as_ref().unwrap().0.event(())
        }
    }
}

#[tokio::test]
async fn actor_create() {
    let sys = ActorSystem::new().await.unwrap();

    assert!(sys.actor_of::<Counter>("valid-name").await.is_ok());

    match sys.actor_of::<Counter>("/").await {
        Ok(_) => panic!("test should not reach here"),
        Err(e) => {
            // test Display
            assert_eq!(
                e.to_string(),
                "Failed to create actor. Cause: Invalid actor name (InvalidName [/] Must contain only a-Z, 0-9, _, or -)"
            );
            assert_eq!(
                format!("{}", e),
                "Failed to create actor. Cause: Invalid actor name (InvalidName [/] Must contain only a-Z, 0-9, _, or -)"
            );
            // test Debug
            assert_eq!(format!("{:?}", e), "InvalidName(InvalidName [/] Must contain only a-Z, 0-9, _, or -)");
            assert_eq!(format!("{:#?}", e), "InvalidName(\n    InvalidName [/] Must contain only a-Z, 0-9, _, or -,\n)");
        }
    }
    assert!(sys.actor_of::<Counter>("*").await.is_err());
    assert!(sys.actor_of::<Counter>("/a/b/c").await.is_err());
    assert!(sys.actor_of::<Counter>("@").await.is_err());
    assert!(sys.actor_of::<Counter>("#").await.is_err());
    assert!(sys.actor_of::<Counter>("abc*").await.is_err());
    assert!(sys.actor_of::<Counter>("!").await.is_err());
}

#[tokio::test]
async fn actor_tell() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<Counter>("me").await.unwrap();

    let (probe, mut listen) = probe();
    actor.tell(TestProbe(probe), None).await;

    for _ in 0..1_000_000 {
        actor.tell(Add, None).await;
    }

    p_assert_eq!(listen, ());
}

#[tokio::test]
async fn actor_try_tell() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<Counter>("me").await.unwrap();
    let actor: BasicActorRef = actor.into();

    let (probe, mut listen) = probe();
    actor
        .try_tell(CounterMsg::TestProbe(TestProbe(probe)), None)
        .await
        .unwrap();

    assert!(actor.try_tell(CounterMsg::Add(Add), None).await.is_ok());
    assert!(actor
        .try_tell("invalid-type".to_string(), None)
        .await
        .is_err());

    for _ in 0..1_000_000 {
        actor.try_tell(CounterMsg::Add(Add), None).await.unwrap();
    }

    p_assert_eq!(listen, ());
}

#[derive(Default)]
struct Parent {
    probe: Option<TestProbe>,
}

#[async_trait::async_trait]
impl Actor for Parent {
    type Msg = TestProbe;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        ctx.actor_of::<Child>("child_a").await.unwrap();

        ctx.actor_of::<Child>("child_b").await.unwrap();

        ctx.actor_of::<Child>("child_c").await.unwrap();

        ctx.actor_of::<Child>("child_d").await.unwrap();
    }

    async fn post_stop(&mut self) {
        // All children have been terminated at this point
        // and we can signal back that the parent has stopped
        self.probe.as_ref().unwrap().0.event(());
    }

    async fn recv(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe = Some(msg);
        self.probe.as_ref().unwrap().0.event(());
    }
}

#[derive(Default)]
struct Child;

#[async_trait::async_trait]
impl Actor for Child {
    type Msg = ();

    async fn recv(
        &mut self,
        _: &Context<Self::Msg>,
        _: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
    }
}

#[tokio::test]
async fn actor_stop() {
    let system = ActorSystem::new().await.unwrap();

    let parent = system.actor_of::<Parent>("parent").await.unwrap();

    let (probe, mut listen) = probe();
    parent.tell(TestProbe(probe), None).await;
    system.print_tree();

    // wait for the probe to arrive at the actor before attempting to stop the actor
    listen.recv().await;

    system.stop(&parent).await;
    p_assert_eq!(listen, ());
}
