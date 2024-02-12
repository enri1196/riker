use riker::actors::*;

#[actor(String, u32)]
#[derive(Clone, Default)]
struct NewActor;

#[async_trait::async_trait]
impl Actor for NewActor {
    type Msg = NewActorMsg;

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Stop
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
        ctx.stop(ctx.myself()).await;
    }
}

#[async_trait::async_trait]
impl Receive<u32> for NewActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: u32,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("u32");
    }
}

#[async_trait::async_trait]
impl Receive<String> for NewActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: String,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("String");
    }
}

#[tokio::test]
async fn run_derived_actor() {
    let sys = ActorSystem::new().await.unwrap();

    let act = sys.actor_of::<NewActor>("act").await.unwrap();

    let msg = NewActorMsg::U32(1);
    act.tell(msg, None).await;

    // wait until all direct children of the user root are terminated
    while sys.user_root().has_children() {
        // in order to lower cpu usage, sleep here
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[actor(String)]
#[derive(Clone, Default)]
struct GenericActor<A: Send + 'static, B>
where
    B: Send + 'static,
{
    _a: A,
    _b: B,
}

#[async_trait::async_trait]
impl<A: Send + 'static, B: Send + 'static> Actor for GenericActor<A, B> {
    type Msg = GenericActorMsg;

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Stop
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
        ctx.stop(ctx.myself()).await;
    }
}

#[async_trait::async_trait]
impl<A: Send + 'static, B: Send + 'static> Receive<String> for GenericActor<A, B> {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: String,
        _send_out: Option<BasicActorRef>,
    ) {
        println!("String");
    }
}

#[tokio::test]
async fn run_derived_generic_actor() {
    let sys = ActorSystem::new().await.unwrap();

    let act = sys.actor_of::<GenericActor<(), ()>>("act").await.unwrap();

    let msg = GenericActorMsg::String("test".to_string());
    act.tell(msg, None).await;

    // wait until all direct children of the user root are terminated
    while sys.user_root().has_children() {
        // in order to lower cpu usage, sleep here
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[derive(Clone, Debug)]
pub struct Message<T> {
    inner: T,
}

#[actor(Message<String>)]
#[derive(Clone, Default)]
struct GenericMsgActor;

#[async_trait::async_trait]
impl Actor for GenericMsgActor {
    type Msg = GenericMsgActorMsg;

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Stop
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
        ctx.stop(ctx.myself()).await;
    }
}

#[async_trait::async_trait]
impl Receive<Message<String>> for GenericMsgActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: Message<String>,
        _sender: Option<BasicActorRef>,
    ) {
        println!("{}", msg.inner);
    }
}

#[tokio::test]
async fn run_generic_message_actor() {
    let sys = ActorSystem::new().await.unwrap();

    let act = sys.actor_of::<GenericMsgActor>("act").await.unwrap();

    let msg = GenericMsgActorMsg::Message(Message {
        inner: "test".to_string(),
    });
    act.tell(msg, None).await;

    // wait until all direct children of the user root are terminated
    while sys.user_root().has_children() {
        // in order to lower cpu usage, sleep here
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

mod test_mod {
    #[derive(Clone, Debug)]
    pub struct GenericMessage<T> {
        pub inner: T,
    }

    #[derive(Clone, Debug)]
    pub struct Message;
}

#[actor(test_mod::GenericMessage<String>, test_mod::Message)]
#[derive(Clone, Default)]
struct PathMsgActor;

#[async_trait::async_trait]
impl Actor for PathMsgActor {
    type Msg = PathMsgActorMsg;

    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Stop
    }

    async fn recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
        ctx.stop(ctx.myself()).await;
    }
}

#[async_trait::async_trait]
impl Receive<test_mod::GenericMessage<String>> for PathMsgActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        msg: test_mod::GenericMessage<String>,
        _sender: Option<BasicActorRef>,
    ) {
        println!("{}", msg.inner);
    }
}

#[async_trait::async_trait]
impl Receive<test_mod::Message> for PathMsgActor {
    async fn receive(
        &mut self,
        _ctx: &Context<Self::Msg>,
        _msg: test_mod::Message,
        _sender: Option<BasicActorRef>,
    ) {
        println!("message");
    }
}

#[tokio::test]
async fn run_path_message_actor() {
    let sys = ActorSystem::new().await.unwrap();

    let act = sys.actor_of::<PathMsgActor>("act").await.unwrap();

    let msg = PathMsgActorMsg::TestModMessage(test_mod::Message);
    act.tell(msg, None).await;

    let generic_msg = PathMsgActorMsg::TestModGenericMessage(test_mod::GenericMessage {
        inner: "test".to_string(),
    });
    act.tell(generic_msg, None).await;

    // wait until all direct children of the user root are terminated
    while sys.user_root().has_children() {
        // in order to lower cpu usage, sleep here
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
