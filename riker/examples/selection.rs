extern crate riker;
use riker::actors::*;
use riker::system::ActorSystem;

use std::time::Duration;

// a simple minimal actor for use in tests
// #[actor(TestProbe)]
#[derive(Default, Debug)]
struct Child;

#[async_trait::async_trait]
impl Actor for Child {
    type Msg = String;

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: {:?} -> got msg: {}", ctx.myself().name(), self, msg);
    }
}

#[derive(Clone, Default, Debug)]
struct SelectTest;

#[async_trait::async_trait]
impl Actor for SelectTest {
    type Msg = String;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        // create first child actor
        let _ = ctx.actor_of::<Child>("child_a").await.unwrap();

        // create second child actor
        let _ = ctx.actor_of::<Child>("child_b").await.unwrap();
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("{}: {:?} -> got msg: {}", ctx.myself().name(), self, msg);
        // up and down: ../select-actor/child_a
        let path = "../select-actor/child_a";
        println!("{}: {:?} -> path: {}", ctx.myself().name(), self, path);
        let sel = ctx.select(path).unwrap();
        sel.try_tell(path.to_string(), None).await;

        // child: child_a
        let path = "child_a";
        println!("{}: {:?} -> path: {}", ctx.myself().name(), self, path);
        let sel = ctx.select(path).unwrap();
        sel.try_tell(path.to_string(), None).await;

        // absolute: /user/select-actor/child_a
        let path = "/user/select-actor/child_a";
        println!("{}: {:?} -> path: {}", ctx.myself().name(), self, path);
        let sel = ctx.select(path).unwrap();
        sel.try_tell(path.to_string(), None).await;

        // absolute all: /user/select-actor/*
        let path = "/user/select-actor/*";
        println!("{}: {:?} -> path: {}", ctx.myself().name(), self, path);
        let sel = ctx.select(path).unwrap();
        sel.try_tell(path.to_string(), None).await;

        // all: *
        let path = "*";
        println!("{}: {:?} -> path: {}", ctx.myself().name(), self, path);
        let sel = ctx.select(path).unwrap();
        sel.try_tell(path.to_string(), None).await;
    }
}

#[tokio::main]
async fn main() {
    let sys = ActorSystem::new().await.unwrap();

    let actor = sys.actor_of::<SelectTest>("select-actor").await.unwrap();

    actor.tell("msg for select-actor", None).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    sys.print_tree();
}
