extern crate riker;
use riker::actors::*;

use std::time::Duration;

// Define the messages we'll use
#[derive(Clone, Debug)]
pub struct Add;

#[derive(Clone, Debug)]
pub struct Sub;

#[derive(Clone, Debug)]
pub struct Print;

// Define the Actor and use the 'actor' attribute
// to specify which messages it will receive
#[actor(Add, Sub, Print)]
struct Counter {
    count: u32,
}

impl ActorFactoryArgs<u32> for Counter {
    fn create_args(count: u32) -> Self {
        Self { count }
    }
}

impl Actor for Counter {
    // we used the #[actor] attribute so CounterMsg is the Msg type
    type Msg = CounterMsg;

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, send_out: Option<BasicActorRef>) {
        // Use the respective Receive<T> implementation
        self.receive(ctx, msg, send_out);
    }
}

impl Receive<Add> for Counter {
    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: Add, _send_out: Option<BasicActorRef>) {
        self.count += 1;
    }
}

impl Receive<Sub> for Counter {
    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: Sub, _send_out: Option<BasicActorRef>) {
        self.count -= 1;
    }
}

impl Receive<Print> for Counter {
    fn receive(&mut self, _ctx: &Context<Self::Msg>, _msg: Print, _send_out: Option<BasicActorRef>) {
        println!("Total counter value: {}", self.count);
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().unwrap();

    let actor = sys.actor_of_args::<Counter, _>("counter", 0).unwrap();
    actor.tell(Add, None);
    actor.tell(Add, None);
    actor.tell(Sub, None);
    actor.tell(Print, None);
    sys.print_tree();
    // force main to wait before exiting program
    tokio::time::sleep(Duration::from_millis(500)).await;
}
