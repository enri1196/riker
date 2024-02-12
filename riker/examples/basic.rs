extern crate riker;
use riker::actors::*;
use std::time::Duration;

#[derive(Default)]
struct MyActor;

#[async_trait::async_trait]
impl Actor for MyActor {
    type Msg = String;

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{} received: {}", ctx.myself().name(), msg);
    }
}

// start the system and create an actor
#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let my_actor = sys.actor_of::<MyActor>("my-actor").await.unwrap();

    my_actor.tell("Hello my actor!".to_string(), None).await;

    tokio::time::sleep(Duration::from_millis(500)).await;
}
