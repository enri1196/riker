use riker::{actors::*, system::SystemCmd};

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

    async fn sys_recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemMsg,
        _send_out: Option<BasicActorRef>,
    ) {
        if let SystemMsg::Command(cmd) = msg {
            match cmd {
                SystemCmd::Stop => ctx.system().stop(ctx.myself()).await,
                SystemCmd::Restart => (),
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
#[async_trait::async_trait]
impl Actor for MyActor {
    type Msg = Command;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        self.child = Some(ctx.actor_of::<Child>("my-child").await.unwrap());
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        match msg {
            Command::KillChild(path) => match ctx.select_ref(path.as_str()) {
                Some(b_act) => ctx.stop(&b_act).await,
                None => (),
            },
            Command::Other(inner_msg) => {
                println!("parent got a message {}", inner_msg);
                self.child.as_ref().unwrap().tell(inner_msg, send_out).await;
            }
        }
    }
}

// start the system and create an actor
#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    println!("Starting actor my-actor");
    let _my_actor = sys.actor_of::<MyActor>("my-actor").await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();

    let _ = match sys.select_ref("/user/my-actor") {
        Some(b_act) => {
            b_act
                .try_tell(Command::Other("CiaoCiao".to_string()), None)
                .await
        }
        None => panic!("No actor found in path /user/my-actor"),
    };

    println!("Killing actor my-actor");
    let b_act = sys.select_ref("/user/my-actor").unwrap();
    b_act
        .try_tell(
            Command::KillChild("/user/my-actor/my-child".to_string()),
            None,
        )
        .await
        .unwrap();
    println!("Actor my-actor should be gone");
    tokio::time::sleep(Duration::from_millis(500)).await;
    sys.print_tree();
}
