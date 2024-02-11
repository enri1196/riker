extern crate riker;
use riker::{actors::*, system::SystemCmd};

use std::time::Duration;

#[derive(Default)]
struct Child;

impl Actor for Child {
    type Msg = String;

    fn recv(&mut self, _ctx: &Context<Self::Msg>, msg: Self::Msg, _send_out: Option<BasicActorRef>) {
        println!("child got a message {}", msg);
    }

    fn sys_recv(&mut self, ctx: &Context<Self::Msg>, msg: SystemMsg, _send_out: Option<BasicActorRef>) {
        if let SystemMsg::Command(cmd) = msg {
            match cmd {
                SystemCmd::Stop => ctx.system().stop(ctx.myself()),
                SystemCmd::Restart => {}
            }
        }
    }
}

#[derive(Default)]
struct MyActor {
    child: Option<ActorRef<String>>,
}

#[derive(Debug, Clone)]
enum Command {
    KillChild(String),
    Other(String),
}

// implement the Actor trait
impl Actor for MyActor {
    type Msg = Command;

    fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.child = Some(ctx.actor_of::<Child>("my-child").unwrap());
    }

    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, send_out: Option<BasicActorRef>) {
        match msg {
            Command::KillChild(path) => {
                ctx.select_ref(path.as_str()).map(|b_act| ctx.stop(&b_act));
            }
            Command::Other(inner_msg) => {
                println!("parent got a message {}", inner_msg);
                self.child.as_ref().unwrap().tell(inner_msg, send_out);
            }
        }
    }
}

// start the system and create an actor
#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().unwrap();

    println!("Starting actor my-actor");
    let _my_actor = sys.actor_of::<MyActor>("my-actor").unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();

    let _ = match sys.select_ref("/user/my-actor") {
        Some(b_act) => b_act.try_tell(Command::Other("CiaoCiao".to_string()), None),
        None => panic!("No actor found in path /user/my-actor"),
    };

    println!("Killing actor my-actor");
    let _select = sys.select_ref("/user/my-actor").map(|b_act| {
        b_act.try_tell(
            Command::KillChild("/user/my-actor/my-child".to_string()),
            None,
        )
    });
    println!("Actor my-actor should be gone");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
