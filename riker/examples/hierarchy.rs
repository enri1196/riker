extern crate riker;
use riker::actors::*;

use std::time::Duration;

#[derive(Default)]
struct Child;

impl Actor for Child {
    type Msg = String;

    fn recv(&mut self, _ctx: &Context<Self::Msg>, msg: Self::Msg, _send_out: Option<BasicActorRef>) {
        println!("child got a message {}", msg);
    }
}

#[derive(Default)]
struct MyActor {
    child: Option<ActorRef<String>>,
}

// implement the Actor trait
impl Actor for MyActor {
    type Msg = String;

    fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.child = Some(ctx.actor_of::<Child>("my-child").unwrap());
    }

    fn recv(&mut self, _ctx: &Context<Self::Msg>, msg: Self::Msg, send_out: Option<BasicActorRef>) {
        println!("parent got a message {}", msg);
        self.child.as_ref().unwrap().tell(msg, send_out);
    }
}

// start the system and create an actor
#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().unwrap();

    let my_actor = sys.actor_of::<MyActor>("my-actor").unwrap();

    my_actor.tell("Hello my actor!".to_string(), None);

    println!("Child not added yet");
    sys.print_tree();

    println!("Child added already");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
