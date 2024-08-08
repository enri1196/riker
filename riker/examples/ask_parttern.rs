use riker::actors::*;

#[derive(Clone, Debug)]
pub struct Panic;

#[derive(Default)]
struct DumbActor;

#[async_trait::async_trait]
impl Actor for DumbActor {
    type Msg = ();

    async fn recv(
        &mut self,
        _: &Context<Self::Msg>,
        _: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        if let Some(sender) = send_out {
            sender
                .try_tell("Sent Hello".to_string(), None)
                .await
                .unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let sys = SystemBuilder::new().name("my-app").create().await.unwrap();
    println!("Created System");
    let sup = sys.actor_of::<DumbActor>("dumb").await.unwrap();
    println!("Created Dumb Actor");
    let str = sup.ask::<String>(()).await.unwrap();
    println!("{str}")
}
