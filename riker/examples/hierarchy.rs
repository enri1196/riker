extern crate riker;
use riker::actors::*;

use std::time::Duration;

#[derive(Default)]
struct Child;

#[async_trait::async_trait]
impl Actor for Child {
    type Msg = String;

    async fn recv(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("child got a message {}", msg);
    }
}

#[derive(Default)]
struct MyActor {
    child: Option<ActorRef<String>>,
}

// implement the Actor trait
#[async_trait::async_trait]
impl Actor for MyActor {
    type Msg = String;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.child = Some(ctx.actor_of::<Child>("my-child").await.unwrap());
    }

    async fn recv(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        println!("parent got a message {}", msg);
        self.child.as_ref().unwrap().tell(msg, send_out).await;
    }
}

// start the system and create an actor
#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let my_actor = sys.actor_of::<MyActor>("my-actor").await.unwrap();

    my_actor.tell("Hello my actor!".to_string(), None).await;

    println!("Child not added yet");
    sys.print_tree();

    println!("Child added already");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
