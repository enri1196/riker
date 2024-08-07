pub(crate) mod logger;
pub(crate) mod timer;

use core::future::Future;
use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::Mutex;

use chrono::prelude::*;
use config::Config;
use tokio::{runtime::Handle, sync::oneshot, task::JoinHandle};
use uuid::Uuid;

use crate::{
    actor::BasicActorRef,
    actor::{props::ActorFactory, *},
    actors::selection::RefSelectionFactory,
    kernel::provider::{create_root, Provider},
    load_config,
    system::logger::*,
    system::timer::*,
    validate::{validate_name, InvalidPath},
    AnyMessage, Message,
};

#[cfg(feature = "serde")]
use serde_json::{json, Value};
use tracing::debug;

use self::actor_ref::BoxedTell;
// Public riker::system API (plus the pub data types in this file)
pub use self::timer::{BasicTimer, ScheduleId, Timer};

#[derive(Clone, Debug)]
pub enum SystemMsg {
    ActorInit,
    Command(SystemCmd),
    Event(SystemEvent),
    Failed(BasicActorRef),
}

unsafe impl Send for SystemMsg {}

#[derive(Clone, Debug)]
pub enum SystemCmd {
    Stop,
    Restart,
}

impl From<SystemCmd> for SystemMsg {
    fn from(val: SystemCmd) -> Self {
        SystemMsg::Command(val)
    }
}

#[derive(Clone, Debug)]
pub enum SystemEvent {
    /// An actor was terminated
    ActorCreated(ActorCreated),

    /// An actor was restarted
    ActorRestarted(ActorRestarted),

    /// An actor was started
    ActorTerminated(ActorTerminated),
}

impl From<SystemEvent> for SystemMsg {
    fn from(val: SystemEvent) -> Self {
        SystemMsg::Event(val)
    }
}

#[derive(Clone, Debug)]
pub struct ActorCreated {
    pub actor: BasicActorRef,
}

#[derive(Clone, Debug)]
pub struct ActorRestarted {
    pub actor: BasicActorRef,
}

#[derive(Clone, Debug)]
pub struct ActorTerminated {
    pub actor: BasicActorRef,
}

impl From<ActorCreated> for SystemEvent {
    fn from(val: ActorCreated) -> Self {
        SystemEvent::ActorCreated(val)
    }
}

impl From<ActorRestarted> for SystemEvent {
    fn from(val: ActorRestarted) -> Self {
        SystemEvent::ActorRestarted(val)
    }
}

impl From<ActorTerminated> for SystemEvent {
    fn from(val: ActorTerminated) -> Self {
        SystemEvent::ActorTerminated(val)
    }
}

impl From<ActorCreated> for SystemMsg {
    fn from(val: ActorCreated) -> Self {
        SystemMsg::Event(SystemEvent::ActorCreated(val))
    }
}

impl From<ActorRestarted> for SystemMsg {
    fn from(val: ActorRestarted) -> Self {
        SystemMsg::Event(SystemEvent::ActorRestarted(val))
    }
}

impl From<ActorTerminated> for SystemMsg {
    fn from(val: ActorTerminated) -> Self {
        SystemMsg::Event(SystemEvent::ActorTerminated(val))
    }
}

#[derive(Clone, Debug)]
pub enum SystemEventType {
    ActorTerminated,
    ActorRestarted,
    ActorCreated,
}

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("Failed to create actor system. Cause: Sub module failed to start ({0})")]
    ModuleFailed(String),
    #[error("Failed to create actor system. Cause: Invalid actor system name ({0})")]
    InvalidName(String),
}

pub struct ProtoSystem {
    id: Uuid,
    name: String,
    pub host: Arc<str>,
    config: Config,
    pub(crate) sys_settings: SystemSettings,
    started_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct SystemBuilder {
    name: Option<String>,
    cfg: Option<Config>,
    exec: Option<Handle>,
}

impl SystemBuilder {
    pub fn new() -> Self {
        SystemBuilder::default()
    }

    pub async fn create(self) -> Result<ActorSystem, SystemError> {
        let name = self.name.unwrap_or_else(|| "riker".to_string());
        let cfg = self.cfg.unwrap_or_else(load_config);
        let exec = self.exec.unwrap_or_else(|| default_exec(&cfg));

        ActorSystem::create(name.as_ref(), exec, cfg).await
    }

    pub fn name(self, name: &str) -> Self {
        SystemBuilder {
            name: Some(name.to_string()),
            ..self
        }
    }

    pub fn cfg(self, cfg: Config) -> Self {
        SystemBuilder {
            cfg: Some(cfg),
            ..self
        }
    }

    pub fn exec(self, exec: Handle) -> Self {
        SystemBuilder {
            exec: Some(exec),
            ..self
        }
    }
}

/// The actor runtime and common services coordinator
///
/// The `ActorSystem` provides a runtime on which actors are executed.
/// It also provides common services such as channels and scheduling.
/// The `ActorSystem` is the heart of a Riker application,
/// starting several threads when it is created. Create only one instance
/// of `ActorSystem` per application.
#[derive(Clone)]
pub struct ActorSystem {
    proto: Arc<ProtoSystem>,
    sys_actors: Option<SysActors>,
    pub exec: Handle,
    pub timer: TimerRef,
    pub sys_channels: Option<SysChannels>,
    pub(crate) provider: Provider,
}

impl ActorSystem {
    /// Create a new `ActorSystem` instance
    ///
    /// Requires a type that implements the `Model` trait.
    pub async fn new() -> Result<ActorSystem, SystemError> {
        let cfg = load_config();
        let exec = default_exec(&cfg);

        ActorSystem::create("riker", exec, cfg).await
    }

    /// Create a new `ActorSystem` instance with provided executor
    ///
    /// Requires a type that implements the `TaskExecutor` trait.
    pub async fn with_executor(exec: Handle) -> Result<ActorSystem, SystemError> {
        let cfg = load_config();

        ActorSystem::create("riker", exec, cfg).await
    }

    /// Create a new `ActorSystem` instance with provided name
    ///
    /// Requires a type that implements the `Model` trait.
    pub async fn with_name(name: &str) -> Result<ActorSystem, SystemError> {
        let cfg = load_config();
        let exec = default_exec(&cfg);

        ActorSystem::create(name, exec, cfg).await
    }

    /// Create a new `ActorSystem` instance bypassing default config behavior
    pub async fn with_config(name: &str, cfg: Config) -> Result<ActorSystem, SystemError> {
        let exec = default_exec(&cfg);

        ActorSystem::create(name, exec, cfg).await
    }

    async fn create(name: &str, exec: Handle, cfg: Config) -> Result<ActorSystem, SystemError> {
        validate_name(name).map_err(|_| SystemError::InvalidName(name.into()))?;
        // Until the logger has started, use println
        debug!("Starting actor system: System[{}]", name);

        let prov = Provider::new();
        let timer = BasicTimer::start(&cfg);

        // 1. create proto system
        let proto = ProtoSystem {
            id: Uuid::new_v4(),
            name: name.to_string(),
            host: Arc::from("localhost"),
            config: cfg.clone(),
            sys_settings: SystemSettings::from(&cfg),
            started_at: Utc::now(),
        };

        // 2. create uninitialized system
        let mut sys = ActorSystem {
            proto: Arc::new(proto),
            exec,
            // event_store: None,
            timer,
            sys_channels: None,
            sys_actors: None,
            provider: prov.clone(),
        };

        // 3. create initial actor hierarchy
        let sys_actors = create_root(&sys).await;
        sys.sys_actors = Some(sys_actors);

        // 4. start system channels
        sys.sys_channels = Some(sys_channels(&prov, &sys).await?);

        // 5. start dead letter logger
        let _dl_logger = sys_actor_of_args::<DeadLetterLogger, _>(
            &prov,
            &sys,
            "dl_logger",
            sys.dead_letters().clone(),
        )
        .await?;

        sys.complete_start();

        debug!("Actor system [{}] [{}] started", sys.id(), name);

        Ok(sys)
    }

    fn complete_start(&self) {
        self.sys_actors.as_ref().unwrap().user.sys_init(self);
    }

    /// Returns the system start date
    pub fn start_date(&self) -> &DateTime<Utc> {
        &self.proto.started_at
    }

    /// Returns the number of seconds since the system started
    pub fn uptime(&self) -> u64 {
        let now = Utc::now();
        now.time()
            .signed_duration_since(self.start_date().time())
            .num_seconds() as u64
    }

    /// Returns the hostname used when the system started
    ///
    /// The host is used in actor addressing.
    ///
    /// Currently not used, but will be once system clustering is introduced.
    pub fn host(&self) -> Arc<str> {
        self.proto.host.clone()
    }

    /// Returns the UUID assigned to the system
    pub fn id(&self) -> Uuid {
        self.proto.id
    }

    /// Returns the name of the system
    pub fn name(&self) -> String {
        self.proto.name.clone()
    }

    pub fn print_tree(&self) {
        fn print_node(sys: &ActorSystem, node: &BasicActorRef, indent: &str) {
            if node.is_root() {
                println!("{}", sys.name());

                for actor in node.children().iter() {
                    print_node(sys, &actor, "");
                }
            } else {
                println!("{}└─ {}", indent, node.name());

                for actor in node.children().iter() {
                    print_node(sys, &actor, &(indent.to_string() + "   "));
                }
            }
        }

        let root = &self.sys_actors.as_ref().unwrap().root;
        print_node(self, root, "");
    }

    #[cfg(feature = "serde")]
    pub fn generate_json(&self) -> Value {
        fn node_to_json(sys: &ActorSystem, node: &BasicActorRef) -> Value {
            if node.is_root() {
                let mut children_json = Vec::new();
                for actor in node.children().iter() {
                    let child_json = node_to_json(sys, &actor);
                    children_json.push(child_json);
                }
                json!({
                    sys.name(): children_json
                })
            } else {
                let mut children_json = Vec::new();
                for actor in node.children().iter() {
                    let child_json = node_to_json(sys, &actor);
                    children_json.push(child_json);
                }
                json!({
                    node.name(): children_json
                })
            }
        }

        let root = &self.sys_actors.as_ref().unwrap().root;
        node_to_json(self, &root)
    }

    #[cfg(feature = "serde")]
    pub fn generate_node_json(&self) -> Value {
        fn node_to_json(
            sys: &ActorSystem,
            node: &BasicActorRef,
            nodes: &mut Vec<Value>,
            edges: &mut Vec<Value>,
        ) {
            nodes.push(json!({ "id": node.name() }));

            for actor in node.children().iter() {
                edges.push(json!({ "from": node.name(), "to": actor.name() }));
                node_to_json(sys, &actor, nodes, edges);
            }
        }

        let root = &self.sys_actors.as_ref().unwrap().root;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        node_to_json(self, &root, &mut nodes, &mut edges);

        json!({
            "nodes": nodes,
            "edges": edges
        })
    }

    /// Returns the system root's actor reference
    #[allow(dead_code)]
    fn root(&self) -> &BasicActorRef {
        &self.sys_actors.as_ref().unwrap().root
    }

    /// Returns the user root actor reference
    pub fn user_root(&self) -> &BasicActorRef {
        &self.sys_actors.as_ref().unwrap().user
    }

    /// Returns the system root actor reference
    pub fn sys_root(&self) -> &BasicActorRef {
        &self.sys_actors.as_ref().unwrap().sysm
    }

    /// Reutrns the temp root actor reference
    pub fn temp_root(&self) -> &BasicActorRef {
        &self.sys_actors.as_ref().unwrap().temp
    }

    /// Returns a reference to the system events channel
    pub fn sys_events(&self) -> &ActorRef<ChannelMsg<SystemEvent>> {
        &self.sys_channels.as_ref().unwrap().sys_events
    }

    /// Returns a reference to the dead letters channel
    pub fn dead_letters(&self) -> &ActorRef<DLChannelMsg> {
        &self.sys_channels.as_ref().unwrap().dead_letters
    }

    pub async fn publish_event(&self, evt: SystemEvent) {
        let topic = Topic::from(&evt);
        self.sys_events()
            .tell(Publish { topic, msg: evt }, None)
            .await;
    }

    /// Returns the `Config` used by the system
    pub fn config(&self) -> &Config {
        &self.proto.config
    }

    pub(crate) fn sys_settings(&self) -> &SystemSettings {
        &self.proto.sys_settings
    }

    /// Create an actor under the system root
    pub async fn sys_actor_of_props<A>(
        &self,
        name: &str,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor,
    {
        self.provider
            .create_actor(props, name, self.sys_root(), self)
            .await
    }

    pub async fn sys_actor_of<A>(
        &self,
        name: &str,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory,
    {
        self.provider
            .create_actor(Props::new::<A>(), name, self.sys_root(), self)
            .await
    }

    pub async fn sys_actor_of_args<A, Args>(
        &self,
        name: &str,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>,
    {
        self.provider
            .create_actor(Props::new_args::<A, _>(args), name, self.sys_root(), self)
            .await
    }

    /// Shutdown the actor system
    ///
    /// Attempts a graceful shutdown of the system and all actors.
    /// Actors will receive a stop message, executing `actor.post_stop`.
    ///
    /// Does not block. Returns a future which is completed when all
    /// actors have successfully stopped.
    pub async fn shutdown(&self) -> Shutdown {
        let (tx, rx) = oneshot::channel::<()>();
        let tx = Arc::new(Mutex::new(Some(tx)));

        self.tmp_actor_of_args::<ShutdownActor, _>(tx)
            .await
            .unwrap();

        rx
    }
}

unsafe impl Send for ActorSystem {}
unsafe impl Sync for ActorSystem {}

#[async_trait::async_trait]
impl ActorRefFactory for ActorSystem {
    async fn actor_of_props<A>(
        &self,
        name: &str,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor,
    {
        self.provider
            .create_actor(props, name, self.user_root(), self)
            .await
    }

    async fn actor_of<A>(&self, name: &str) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory,
    {
        self.provider
            .create_actor(Props::new::<A>(), name, self.user_root(), self)
            .await
    }

    async fn actor_of_args<A, Args>(
        &self,
        name: &str,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>,
    {
        self.provider
            .create_actor(Props::new_args::<A, _>(args), name, self.user_root(), self)
            .await
    }

    async fn stop(&self, actor: impl SysTell) {
        actor.sys_tell(SystemCmd::Stop.into()).await;
    }
}

#[async_trait::async_trait]
impl ActorRefFactory for &ActorSystem {
    async fn actor_of_props<A>(
        &self,
        name: &str,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor,
    {
        self.provider
            .create_actor(props, name, self.user_root(), self)
            .await
    }

    async fn actor_of<A>(&self, name: &str) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory,
    {
        self.provider
            .create_actor(Props::new::<A>(), name, self.user_root(), self)
            .await
    }

    async fn actor_of_args<A, Args>(
        &self,
        name: &str,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>,
    {
        self.provider
            .create_actor(Props::new_args::<A, _>(args), name, self.user_root(), self)
            .await
    }

    async fn stop(&self, actor: impl SysTell) {
        actor.sys_tell(SystemCmd::Stop.into()).await;
    }
}

#[async_trait::async_trait]
impl TmpActorRefFactory for ActorSystem {
    async fn tmp_actor_of_props<A>(
        &self,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor,
    {
        let name = format!("{}", rand::random::<u64>());
        self.provider
            .create_actor(props, &name, self.temp_root(), self)
            .await
    }

    async fn tmp_actor_of<A>(&self) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory,
    {
        let name = format!("{}", rand::random::<u64>());
        self.provider
            .create_actor(Props::new::<A>(), &name, self.temp_root(), self)
            .await
    }

    async fn tmp_actor_of_args<A, Args>(
        &self,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>,
    {
        let name = format!("{}", rand::random::<u64>());
        self.provider
            .create_actor(Props::new_args::<A, _>(args), &name, self.temp_root(), self)
            .await
    }
}

impl ActorSelectionFactory for ActorSystem {
    fn select(&self, path: &str) -> Result<ActorSelection, InvalidPath> {
        let anchor = self.user_root();
        let (anchor, path_str) = if path.starts_with('/') {
            let anchor = self.user_root();
            let anchor_path = format!("{}/", anchor.path());
            let path = path.to_string().replace(&anchor_path, "");

            (anchor, path)
        } else {
            (anchor, path.to_string())
        };

        ActorSelection::new(
            anchor.clone(),
            // self.dead_letters(),
            path_str,
        )
    }
}

impl RefSelectionFactory for ActorSystem {
    fn select_ref(&self, path: &str) -> Option<BasicActorRef> {
        fn find_actor_by_path_recursive(root: &BasicActorRef, path: &str) -> Option<BasicActorRef> {
            if root.path() == path {
                Some(root.clone())
            } else if root.has_children() {
                root.children()
                    .iter()
                    .find_map(|act| find_actor_by_path_recursive(&act, path))
            } else {
                None
            }
        }

        find_actor_by_path_recursive(self.user_root(), path)
    }
}

pub trait Run {
    fn run<Fut>(&self, future: Fut) -> JoinHandle<Fut::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send;
}

impl Run for ActorSystem {
    fn run<Fut>(&self, future: Fut) -> JoinHandle<Fut::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send,
    {
        self.exec.spawn(future)
    }
}

impl fmt::Debug for ActorSystem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = self.name();
        let start_date = self.start_date();
        let uptime = self.uptime();
        write!(
            f,
            "ActorSystem[Name: {name}, Start Time: {start_date}, Uptime: {uptime} seconds]"
        )
    }
}

#[async_trait::async_trait]
impl Timer for ActorSystem {
    async fn schedule<T, M>(
        &self,
        initial_delay: Duration,
        interval: Duration,
        receiver: ActorRef<M>,
        send_out: Option<BasicActorRef>,
        msg: T,
    ) -> ScheduleId
    where
        T: Message + Into<M>,
        M: Message,
    {
        let id = Uuid::new_v4();
        let msg: M = msg.into();

        let job = RepeatJob {
            id,
            send_at: Instant::now() + initial_delay,
            interval,
            receiver: receiver.into(),
            send_out,
            msg: AnyMessage::new(msg, false),
        };

        let _ = self.timer.send(Job::Repeat(job)).await;
        id
    }

    async fn schedule_once<T, M>(
        &self,
        delay: Duration,
        receiver: ActorRef<M>,
        send_out: Option<BasicActorRef>,
        msg: T,
    ) -> ScheduleId
    where
        T: Message + Into<M>,
        M: Message,
    {
        let id = Uuid::new_v4();
        let msg: M = msg.into();

        let job = OnceJob {
            id,
            send_at: Instant::now() + delay,
            receiver: receiver.into(),
            send_out,
            msg: AnyMessage::new(msg, true),
        };

        let _ = self.timer.send(Job::Once(job)).await;
        id
    }

    async fn schedule_at_time<T, M>(
        &self,
        time: DateTime<Utc>,
        receiver: ActorRef<M>,
        send_out: Option<BasicActorRef>,
        msg: T,
    ) -> ScheduleId
    where
        T: Message + Into<M>,
        M: Message,
    {
        let delay = std::cmp::max(time.timestamp() - Utc::now().timestamp(), 0_i64);
        let delay = Duration::from_secs(delay as u64);

        let id = Uuid::new_v4();
        let msg: M = msg.into();

        let job = OnceJob {
            id,
            send_at: Instant::now() + delay,
            receiver: receiver.into(),
            send_out,
            msg: AnyMessage::new(msg, true),
        };

        let _ = self.timer.send(Job::Once(job)).await;
        id
    }

    async fn cancel_schedule(&self, id: Uuid) {
        let _ = self.timer.send(Job::Cancel(id)).await;
    }
}

// helper functions
#[allow(unused)]
async fn sys_actor_of_props<A>(
    prov: &Provider,
    sys: &ActorSystem,
    name: &str,
    props: BoxActorProd<A>,
) -> Result<ActorRef<A::Msg>, SystemError>
where
    A: Actor,
{
    prov.create_actor(props, name, sys.sys_root(), sys)
        .await
        .map_err(|_| SystemError::ModuleFailed(name.into()))
}

async fn sys_actor_of<A>(
    prov: &Provider,
    sys: &ActorSystem,
    name: &str,
) -> Result<ActorRef<<A as Actor>::Msg>, SystemError>
where
    A: ActorFactory,
{
    prov.create_actor(Props::new::<A>(), name, sys.sys_root(), sys)
        .await
        .map_err(|_| SystemError::ModuleFailed(name.into()))
}

async fn sys_actor_of_args<A, Args>(
    prov: &Provider,
    sys: &ActorSystem,
    name: &str,
    args: Args,
) -> Result<ActorRef<<A as Actor>::Msg>, SystemError>
where
    Args: ActorArgs,
    A: ActorFactoryArgs<Args>,
{
    prov.create_actor(Props::new_args::<A, _>(args), name, sys.sys_root(), sys)
        .await
        .map_err(|_| SystemError::ModuleFailed(name.into()))
}

async fn sys_channels(prov: &Provider, sys: &ActorSystem) -> Result<SysChannels, SystemError> {
    let sys_events = sys_actor_of::<EventsChannel>(prov, sys, "sys_events").await?;
    let dead_letters = sys_actor_of::<Channel<DeadLetter>>(prov, sys, "dead_letters").await?;

    // subscribe the dead_letters channel to actor terminated events
    // so that any future subscribed actors that terminate are automatically
    // unsubscribed from the dead_letters channel
    // let msg = ChannelMsg::Subscribe(SysTopic::ActorTerminated.into(), dl.clone());
    // es.tell(msg, None);

    Ok(SysChannels {
        sys_events,
        dead_letters,
    })
}

pub struct SystemSettings {
    pub msg_process_limit: u32,
}

impl<'a> From<&'a Config> for SystemSettings {
    fn from(config: &Config) -> Self {
        SystemSettings {
            msg_process_limit: config.get_int("mailbox.msg_process_limit").unwrap() as u32,
        }
    }
}

fn default_exec(_cfg: &Config) -> Handle {
    Handle::current()
}

#[derive(Clone)]
pub struct SysActors {
    pub root: BasicActorRef,
    pub user: BasicActorRef,
    pub sysm: BasicActorRef,
    pub temp: BasicActorRef,
}

#[derive(Clone)]
pub struct SysChannels {
    pub sys_events: ActorRef<ChannelMsg<SystemEvent>>,
    pub dead_letters: ActorRef<DLChannelMsg>,
}

pub type Shutdown = oneshot::Receiver<()>;

#[derive(Clone)]
struct ShutdownActor {
    tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl ActorFactoryArgs<Arc<Mutex<Option<oneshot::Sender<()>>>>> for ShutdownActor {
    fn create_args(tx: Arc<Mutex<Option<oneshot::Sender<()>>>>) -> Self {
        ShutdownActor::new(tx)
    }
}

impl ShutdownActor {
    fn new(tx: Arc<Mutex<Option<oneshot::Sender<()>>>>) -> Self {
        ShutdownActor { tx }
    }
}

#[async_trait::async_trait]
impl Actor for ShutdownActor {
    type Msg = SystemEvent;

    async fn pre_start(&mut self, ctx: &Context<Self::Msg>) {
        let sub = Subscribe {
            topic: SysTopic::ActorTerminated.into(),
            actor: BoxedTell(Arc::new(ctx.myself().clone())),
        };
        ctx.system().sys_events().tell(sub, None).await;

        // todo this is prone to failing since there is no
        // confirmation that ShutdownActor has subscribed to
        // the ActorTerminated events yet.
        // It may be that the user root actor is Sterminated
        // before the subscription is complete.

        // tokio::time::sleep_ms(1000);
        // send stop to all /user children
        ctx.system().stop(ctx.system().user_root()).await;
    }

    async fn sys_recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemMsg,
        sender: Option<BasicActorRef>,
    ) {
        if let SystemMsg::Event(SystemEvent::ActorTerminated(terminated)) = msg {
            self.receive(ctx, terminated, sender).await;
        }
    }

    async fn recv(&mut self, _: &Context<Self::Msg>, _: Self::Msg, _: Option<BasicActorRef>) {}
}

#[async_trait::async_trait]
impl Receive<ActorTerminated> for ShutdownActor {
    async fn receive(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: ActorTerminated,
        _sender: Option<BasicActorRef>,
    ) {
        if &msg.actor == ctx.system().user_root() {
            let mut tx = self.tx.lock().await;
            if let Some(tx) = tx.take() {
                tx.send(()).unwrap();
            }
        }
    }
}
