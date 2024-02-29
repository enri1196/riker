use riker::actors::*;

#[tokio::test]
async fn system_create() {
    assert!(ActorSystem::new().await.is_ok());
    assert!(ActorSystem::with_name("valid-name").await.is_ok());

    assert!(ActorSystem::with_name("/").await.is_err());
    assert!(ActorSystem::with_name("*").await.is_err());
    assert!(ActorSystem::with_name("/a/b/c").await.is_err());
    assert!(ActorSystem::with_name("@").await.is_err());
    assert!(ActorSystem::with_name("#").await.is_err());
    assert!(ActorSystem::with_name("abc*").await.is_err());
}

struct ShutdownTest {
    level: u32,
}

impl ActorFactoryArgs<u32> for ShutdownTest {
    fn create_args(level: u32) -> Self {
        ShutdownTest { level }
    }
}

#[async_trait::async_trait]
impl Actor for ShutdownTest {
    type Msg = ();

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        if self.level < 10 {
            ctx.actor_of_args::<ShutdownTest, _>(
                format!("test-actor-{}", self.level + 1).as_str(),
                self.level + 1,
            )
            .await
            .unwrap();
        }
    }

    async fn recv(
        &mut self,
        _: &Context<Self::Msg>,
        _: Self::Msg,
        _send_out: Option<BasicActorRef>,
    ) {
    }
}

#[tokio::test]
async fn system_shutdown() {
    let sys = ActorSystem::new().await.unwrap();

    let _ = sys
        .actor_of_args::<ShutdownTest, _>("test-actor-1", 1)
        .await
        .unwrap();

    sys.shutdown().await;
}

#[tokio::test]
async fn system_futures_exec() {
    let sys = ActorSystem::new().await.unwrap();

    for i in 0..100 {
        let f = sys.run(async move { format!("some_val_{}", i) });
        let result = f.await;
        assert_eq!(result.unwrap(), format!("some_val_{}", i));
    }
}

#[tokio::test]
async fn system_futures_panic() {
    let sys = ActorSystem::new().await.unwrap();

    for _ in 0..100 {
        let _ = sys.run(async move {
            panic!("// TEST PANIC // TEST PANIC // TEST PANIC //");
        });
    }

    for i in 0..100 {
        let f = sys.run(async move { format!("some_val_{}", i) });
        let result = f.await;
        assert_eq!(result.unwrap(), format!("some_val_{}", i));
    }
}

#[tokio::test]
async fn system_load_app_config() {
    let sys = ActorSystem::new().await.unwrap();

    assert_eq!(sys.config().get_int("app.some_setting").unwrap() as i64, 1);
}

#[tokio::test]
async fn system_builder() {
    let sys = SystemBuilder::new().create().await.unwrap();
    sys.shutdown().await;

    let sys = SystemBuilder::new().name("my-sys").create().await.unwrap();
    sys.shutdown().await;
}
