use riker::actors::*;

use riker::system::ActorSystem;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PowerStatus;

#[actor(PowerStatus)]
struct GpsActor {
    chan: ChannelRef<PowerStatus>,
}

impl ActorFactoryArgs<ChannelRef<PowerStatus>> for GpsActor {
    fn create_args(chan: ChannelRef<PowerStatus>) -> Self {
        GpsActor { chan }
    }
}

#[async_trait::async_trait]
impl Actor for GpsActor {
    type Msg = GpsActorMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let topic = Topic::from("my-topic");

        println!(
            "{}: pre_start subscribe to {:?}",
            ctx.myself().name(),
            topic
        );
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        self.chan.tell(Subscribe { actor: sub, topic }, None).await;
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
impl Receive<PowerStatus> for GpsActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: PowerStatus,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: -> got msg: {:?}", ctx.myself().name(), msg);
    }
}

#[actor(PowerStatus)]
struct NavigationActor {
    chan: ChannelRef<PowerStatus>,
}

impl ActorFactoryArgs<ChannelRef<PowerStatus>> for NavigationActor {
    fn create_args(chan: ChannelRef<PowerStatus>) -> Self {
        NavigationActor { chan }
    }
}

#[async_trait::async_trait]
impl Actor for NavigationActor {
    type Msg = NavigationActorMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let topic = Topic::from("my-topic");

        println!(
            "{}: pre_start subscribe to {:?}",
            ctx.myself().name(),
            topic
        );
        let sub = BoxedTell(Arc::new(ctx.myself().clone()));
        self.chan.tell(Subscribe { actor: sub, topic }, None).await;
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
impl Receive<PowerStatus> for NavigationActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: PowerStatus,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: -> got msg: {:?}", ctx.myself().name(), msg);
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();
    let chan: ChannelRef<PowerStatus> = Channel::new("power-status", &sys).await.unwrap();

    sys.actor_of_args::<GpsActor, _>("gps-actor", chan.clone())
        .await
        .unwrap();
    sys.actor_of_args::<NavigationActor, _>("navigation-actor", chan.clone())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    // sys.print_tree();
    let topic = Topic::from("my-topic");
    println!(
        "Sending PowerStatus message to all subscribers and {:?}",
        topic
    );
    chan.tell(
        Publish {
            msg: PowerStatus,
            topic,
        },
        None,
    )
    .await;
    // sleep another half seconds to process messages
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
