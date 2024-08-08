use riker::actors::*;

use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct Panic;

#[actor(Panic)]
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

// *** Publish test ***
#[actor(SystemEvent)]
#[derive(Default)]
struct SystemActor;

#[async_trait::async_trait]
impl Actor for SystemActor {
    type Msg = SystemActorMsg;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let topic = Topic::from("*");

        println!(
            "{}: pre_start subscribe to topic {:?}",
            ctx.myself().name(),
            topic
        );
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
impl Receive<SystemEvent> for SystemActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemEvent,
        _send_out: Option<BasicActorRef>,
    ) {
        print!("{}: -> got system msg: {:?} ", ctx.myself().name(), msg);
        match msg {
            SystemEvent::ActorCreated(created) => {
                println!("path: {}", created.actor.path());
            }
            SystemEvent::ActorRestarted(restarted) => {
                println!("path: {}", restarted.actor.path());
            }
            SystemEvent::ActorTerminated(terminated) => {
                println!("path: {}", terminated.actor.path());
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let _sub = sys.actor_of::<SystemActor>("system-actor").await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Creating dump actor");
    let dumb = sys.actor_of::<DumbActor>("dumb-actor").await.unwrap();

    // sleep another half seconds to process messages
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Force restart of actor
    println!("Send Panic message to dump actor to get restart");
    dumb.tell(Panic, None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Stopping dump actor");
    sys.stop(&dumb).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
