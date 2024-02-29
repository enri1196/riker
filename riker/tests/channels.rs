#[macro_use]
extern crate riker_testkit;

use std::sync::Arc;

use riker::actors::*;

use riker_testkit::probe::channel::{probe, ChannelProbe};
use riker_testkit::probe::{Probe, ProbeReceive};

#[derive(Clone, Debug)]
pub struct TestProbe(ChannelProbe<(), ()>);

#[derive(Clone, Debug)]
pub struct SomeMessage;

// *** Publish test ***
#[actor(TestProbe, SomeMessage)]
struct Subscriber {
    probe: Option<TestProbe>,
    chan: ChannelRef<SomeMessage>,
    topic: Topic,
}

impl ActorFactoryArgs<(ChannelRef<SomeMessage>, Topic)> for Subscriber {
    fn create_args((chan, topic): (ChannelRef<SomeMessage>, Topic)) -> Self {
        Subscriber {
            probe: None,
            chan,
            topic,
        }
    }
}

#[async_trait::async_trait]
impl Actor for Subscriber {
    type Msg = SubscriberMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        self.chan
            .tell(
                Subscribe {
                    actor: sub,
                    topic: self.topic.clone(),
                },
                None,
            )
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
impl Receive<TestProbe> for Subscriber {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        msg.0.event(());
        self.probe = Some(msg);
    }
}

#[async_trait::async_trait]
impl Receive<SomeMessage> for Subscriber {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: SomeMessage,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe.as_ref().unwrap().0.event(());
    }
}

#[tokio::test]
async fn channel_publish() {
    let sys = ActorSystem::new().await.unwrap();

    // Create the channel we'll be using
    let chan: ChannelRef<SomeMessage> = channel("my-chan", &sys).await.unwrap();

    // The topic we'll be publishing to. Endow our subscriber test actor with this.
    // On Subscriber's pre_start it will subscribe to this channel+topic
    let topic = Topic::from("my-topic");
    let sub = sys
        .actor_of_args::<Subscriber, _>("sub-actor", (chan.clone(), topic.clone()))
        .await
        .unwrap();

    let (probe, mut listen) = probe();
    sub.tell(TestProbe(probe), None).await;

    // wait for the probe to arrive at the actor before publishing message
    listen.recv().await;

    // Publish a test message
    chan.tell(
        Publish {
            msg: SomeMessage,
            topic,
        },
        None,
    )
    .await;

    p_assert_eq!(listen, ());
}

#[tokio::test]
async fn channel_publish_subscribe_all() {
    let sys = ActorSystem::new().await.unwrap();

    // Create the channel we'll be using
    let chan: ChannelRef<SomeMessage> = channel("my-chan", &sys).await.unwrap();

    // The '*' All topic. Endow our subscriber test actor with this.
    // On Subscriber's pre_start it will subscribe to all topics on this channel.
    let topic = Topic::from("*");
    let sub = sys
        .actor_of_args::<Subscriber, _>("sub-actor", (chan.clone(), topic))
        .await
        .unwrap();

    let (probe, mut listen) = probe();
    sub.tell(TestProbe(probe), None).await;

    // wait for the probe to arrive at the actor before publishing message
    listen.recv().await;

    // Publish a test message to topic "topic-1"
    chan.tell(
        Publish {
            msg: SomeMessage,
            topic: "topic-1".into(),
        },
        None,
    )
    .await;

    // Publish a test message to topic "topic-2"
    chan.tell(
        Publish {
            msg: SomeMessage,
            topic: "topic-2".into(),
        },
        None,
    )
    .await;

    // Publish a test message to topic "topic-3"
    chan.tell(
        Publish {
            msg: SomeMessage,
            topic: "topic-3".into(),
        },
        None,
    )
    .await;

    // Expecting three probe events
    p_assert_eq!(listen, ());
    p_assert_eq!(listen, ());
    p_assert_eq!(listen, ());
}

#[derive(Clone, Debug)]
pub struct Panic;

#[actor(Panic, SomeMessage)]
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
impl Receive<Panic> for DumbActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: Panic,
        _send_out: Option<BasicActorRef>,
    ) {
        panic!("// TEST PANIC // TEST PANIC // TEST PANIC //");
    }
}

#[async_trait::async_trait]
impl Receive<SomeMessage> for DumbActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: SomeMessage,
        _send_out: Option<BasicActorRef>,
    ) {

        // Intentionally left blank
    }
}

// We must wrap SystemEvent in a type defined in this test crate
// so we can implement traits on it
#[derive(Clone, Debug)]
struct SysEvent(SystemEvent);

// *** Event stream test ***
#[actor(TestProbe, SystemEvent)]
#[derive(Default)]
struct EventSubscriber {
    probe: Option<TestProbe>,
}

#[async_trait::async_trait]
impl Actor for EventSubscriber {
    type Msg = EventSubscriberMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        // subscribe
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        ctx.system()
            .sys_events()
            .tell(
                Subscribe {
                    actor: sub,
                    topic: "*".into(),
                },
                None,
            )
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

    async fn sys_recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemMsg,
        send_out: Option<BasicActorRef>,
    ) {
        if let SystemMsg::Event(evt) = msg {
            self.receive(ctx, evt, send_out).await;
        }
    }
}

#[async_trait::async_trait]
impl Receive<TestProbe> for EventSubscriber {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        msg.0.event(());
        self.probe = Some(msg);
    }
}

#[async_trait::async_trait]
impl Receive<SystemEvent> for EventSubscriber {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: SystemEvent,
        _send_out: Option<BasicActorRef>,
    ) {
        match msg {
            SystemEvent::ActorCreated(created) => {
                if created.actor.path() == "/user/dumb-actor" {
                    self.probe.as_ref().unwrap().0.event(())
                }
            }
            SystemEvent::ActorRestarted(restarted) => {
                if restarted.actor.path() == "/user/dumb-actor" {
                    self.probe.as_ref().unwrap().0.event(())
                }
            }
            SystemEvent::ActorTerminated(terminated) => {
                if terminated.actor.path() == "/user/dumb-actor" {
                    self.probe.as_ref().unwrap().0.event(())
                }
            }
        }
    }
}

#[tokio::test]
async fn channel_system_events() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<EventSubscriber>("event-sub").await.unwrap();

    let (probe, mut listen) = probe();
    actor.tell(TestProbe(probe), None).await;

    // wait for the probe to arrive at the actor before attempting
    // create, restart and stop
    listen.recv().await;

    // Create an actor
    let dumb = sys.actor_of::<DumbActor>("dumb-actor").await.unwrap();
    // ActorCreated event was received
    p_assert_eq!(listen, ());

    // Force restart of actor
    dumb.tell(Panic, None).await;
    // ActorRestarted event was received
    p_assert_eq!(listen, ());

    // Terminate actor
    sys.stop(&dumb).await;
    // ActorTerminated event was receive
    p_assert_eq!(listen, ());
}

// *** Dead letters test ***
#[actor(TestProbe, DeadLetter)]
#[derive(Default)]
struct DeadLetterSub {
    probe: Option<TestProbe>,
}

#[async_trait::async_trait]
impl Actor for DeadLetterSub {
    type Msg = DeadLetterSubMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        // subscribe to dead_letters
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        ctx.system()
            .dead_letters()
            .tell(
                Subscribe {
                    actor: sub,
                    topic: "*".into(),
                },
                None,
            )
            .await;
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
impl Receive<TestProbe> for DeadLetterSub {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        msg.0.event(());
        self.probe = Some(msg);
    }
}

#[async_trait::async_trait]
impl Receive<DeadLetter> for DeadLetterSub {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: DeadLetter,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe.as_ref().unwrap().0.event(());
    }
}

#[tokio::test]
async fn channel_dead_letters() {
    let sys = ActorSystem::new().await.unwrap();
    let actor = sys
        .actor_of::<DeadLetterSub>("dl-subscriber")
        .await
        .unwrap();

    let (probe, mut listen) = probe();
    actor.tell(TestProbe(probe), None).await;

    // wait for the probe to arrive at the actor before attempting to stop the actor
    listen.recv().await;

    let dumb = sys.actor_of::<DumbActor>("dumb-actor").await.unwrap();

    // immediately stop the actor and attempt to send a message
    sys.stop(&dumb).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    dumb.tell(SomeMessage, None).await;

    p_assert_eq!(listen, ());
}
