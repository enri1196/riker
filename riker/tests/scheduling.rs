#[macro_use]
extern crate riker_testkit;

use riker::actors::*;

use riker_testkit::probe::channel::{probe, ChannelProbe};
use riker_testkit::probe::{Probe, ProbeReceive};

use chrono::{Duration as CDuration, Utc};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct TestProbe(ChannelProbe<(), ()>);

#[derive(Clone, Debug)]
pub struct SomeMessage;

#[actor(TestProbe, SomeMessage)]
#[derive(Default)]
struct ScheduleOnce {
    probe: Option<TestProbe>,
}

#[async_trait::async_trait]
impl Actor for ScheduleOnce {
    type Msg = ScheduleOnceMsg;

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
impl Receive<TestProbe> for ScheduleOnce {
    async fn receive(
        &mut self,
        ctx: &Context<ScheduleOnceMsg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe = Some(msg);
        // reschedule an Empty to be sent to myself()
        ctx.schedule_once(
            Duration::from_millis(200),
            ctx.myself().clone(),
            None,
            SomeMessage,
        )
        .await;
    }
}

#[async_trait::async_trait]
impl Receive<SomeMessage> for ScheduleOnce {
    async fn receive(
        &mut self,
        _ctx: &Context<ScheduleOnceMsg>,
        _msg: SomeMessage,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe.as_ref().unwrap().0.event(());
    }
}

#[tokio::test]
async fn schedule_once() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<ScheduleOnce>("schedule-once").await.unwrap();

    let (probe, mut listen) = probe();

    // use scheduler to set up probe
    sys.schedule_once(Duration::from_millis(200), actor, None, TestProbe(probe))
        .await;
    p_assert_eq!(listen, ());
}

#[tokio::test]
async fn schedule_at_time() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<ScheduleOnce>("schedule-once").await.unwrap();

    let (probe, mut listen) = probe();

    // use scheduler to set up probe at a specific time
    let schedule_at = Utc::now() + CDuration::try_milliseconds(200).unwrap();
    sys.schedule_at_time(schedule_at, actor, None, TestProbe(probe))
        .await;
    p_assert_eq!(listen, ());
}

// *** Schedule repeat test ***
#[actor(TestProbe, SomeMessage)]
#[derive(Default)]
struct ScheduleRepeat {
    probe: Option<TestProbe>,
    counter: u32,
    schedule_id: Option<ScheduleId>,
}

#[async_trait::async_trait]
impl Actor for ScheduleRepeat {
    type Msg = ScheduleRepeatMsg;

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
impl Receive<TestProbe> for ScheduleRepeat {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: TestProbe,
        _send_out: Option<BasicActorRef>,
    ) {
        self.probe = Some(msg);
        // schedule Message to be repeatedly sent to myself
        // and store the job id to cancel it later
        let id = ctx
            .schedule(
                Duration::from_millis(200),
                Duration::from_millis(200),
                ctx.myself().clone(),
                None,
                SomeMessage,
            )
            .await;
        self.schedule_id = Some(id);
    }
}

#[async_trait::async_trait]
impl Receive<SomeMessage> for ScheduleRepeat {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        _msg: SomeMessage,
        _send_out: Option<BasicActorRef>,
    ) {
        if self.counter == 5 {
            ctx.cancel_schedule(self.schedule_id.unwrap()).await;
            self.probe.as_ref().unwrap().0.event(());
        } else {
            self.counter += 1;
        }
    }
}

#[tokio::test]
async fn schedule_repeat() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys
        .actor_of::<ScheduleRepeat>("schedule-repeat")
        .await
        .unwrap();

    let (probe, mut listen) = probe();

    actor.tell(TestProbe(probe), None).await;

    p_assert_eq!(listen, ());
}
