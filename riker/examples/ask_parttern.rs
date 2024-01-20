extern crate riker;
use riker::{actors::*, ask::ask_ref};

#[derive(Clone, Debug)]
pub struct Panic;

#[derive(Default)]
struct DumbActor;

impl Actor for DumbActor {
    type Msg = ();

    fn recv(&mut self, _: &Context<Self::Msg>, _: Self::Msg, sender: Sender) {
        if let Some(sender) = sender {
            sender.try_tell("Sent Hello".to_string(), None).unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let sys = SystemBuilder::new().name("my-app").create().unwrap();

    let _sup = sys.actor_of::<DumbActor>("dumb").unwrap();
    let ac_ref = sys.select_ref("/user/dumb").unwrap();
    let str = ask_ref::<_, String>(sys, &ac_ref, ()).await.unwrap();
    println!("{str}")
}
