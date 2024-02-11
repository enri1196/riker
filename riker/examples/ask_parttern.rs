extern crate riker;
use riker::{actors::*, ask::Ask};

#[derive(Clone, Debug)]
pub struct Panic;

#[derive(Default)]
struct DumbActor;

impl Actor for DumbActor {
    type Msg = ();

    fn recv(&mut self, _: &Context<Self::Msg>, _: Self::Msg, send_out: Option<BasicActorRef>) {
        if let Some(sender) = send_out {
            sender.try_tell("Sent Hello".to_string(), None).unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let sys = SystemBuilder::new().name("my-app").create().unwrap();

    let sup = sys.actor_of::<DumbActor>("dumb").unwrap();
    let str = sup.ask::<String>(()).await.unwrap();
    println!("{str}")
}
