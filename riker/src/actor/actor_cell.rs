use core::future::Future;
use std::{
    fmt,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use chrono::prelude::*;
use dashmap::DashMap;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    actor::{props::ActorFactory, *},
    kernel::{
        kernel_ref::{dispatch, dispatch_any, KernelRef},
        mailbox::{AnyEnqueueError, AnySender, MailboxSender},
    },
    system::{
        timer::{Job, OnceJob, RepeatJob, ScheduleId, Timer},
        ActorSystem, Run, SystemCmd, SystemMsg,
    },
    validate::InvalidPath,
    AnyMessage, Envelope, Message,
};

#[derive(Clone)]
pub struct ActorCell {
    inner: Arc<ActorCellInner>,
}

#[derive(Clone)]
struct ActorCellInner {
    uri: ActorUri,
    parent: Option<BasicActorRef>,
    children: Children,
    // is_remote: bool,
    is_terminating: Arc<AtomicBool>,
    is_restarting: Arc<AtomicBool>,
    // status: Arc<AtomicUsize>,
    kernel: Option<KernelRef>,
    system: ActorSystem,
    mailbox: Arc<dyn AnySender>,
    sys_mailbox: MailboxSender<SystemMsg>,
}

impl ActorCell {
    /// Constructs a new `ActorCell`
    pub(crate) fn new(
        uri: ActorUri,
        parent: Option<BasicActorRef>,
        system: &ActorSystem,
        mailbox: Arc<dyn AnySender>,
        sys_mailbox: MailboxSender<SystemMsg>,
    ) -> ActorCell {
        ActorCell {
            inner: Arc::new(ActorCellInner {
                uri,
                parent,
                children: Children::new(),
                // is_remote: false,
                is_terminating: Arc::new(AtomicBool::new(false)),
                is_restarting: Arc::new(AtomicBool::new(false)),
                // status: Arc::new(AtomicUsize::new(0)),
                kernel: None,
                system: system.clone(),
                mailbox,
                sys_mailbox,
            }),
        }
    }

    pub(crate) fn init(self, kernel: &KernelRef) -> ActorCell {
        let inner = ActorCellInner {
            kernel: Some(kernel.clone()),
            ..self.inner.deref().clone()
        };

        ActorCell {
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn kernel(&self) -> &KernelRef {
        self.inner.kernel.as_ref().unwrap()
    }

    pub(crate) fn myself(&self) -> BasicActorRef {
        BasicActorRef { cell: self.clone() }
    }

    pub(crate) fn uri(&self) -> &ActorUri {
        &self.inner.uri
    }

    pub(crate) fn parent(&self) -> BasicActorRef {
        self.inner.parent.as_ref().unwrap().clone()
    }

    pub fn has_children(&self) -> bool {
        self.inner.children.len() > 0
    }

    pub(crate) fn children(&self) -> &Children {
        &self.inner.children
    }

    pub(crate) fn system(&self) -> &ActorSystem {
        &self.inner.system
    }

    pub(crate) fn user_root(&self) -> BasicActorRef {
        self.inner.system.user_root().clone()
    }

    pub(crate) fn is_root(&self) -> bool {
        self.myself().path() == "/"
    }

    pub fn is_user(&self) -> bool {
        self.inner.system.user_root().is_child(&self.myself())
    }

    pub(crate) async fn send_any_msg(
        &self,
        msg: &mut AnyMessage,
        send_out: Option<BasicActorRef>,
    ) -> Result<(), AnyEnqueueError> {
        let mb = &self.inner.mailbox;
        let k = self.kernel();

        dispatch_any(msg, send_out, mb, k, &self.inner.system).await
    }

    pub(crate) async fn send_sys_msg(
        &self,
        msg: Envelope<SystemMsg>,
    ) -> MsgResult<Envelope<SystemMsg>> {
        let mb = &self.inner.sys_mailbox;

        let k = self.kernel();
        dispatch(msg, mb, k, &self.inner.system).await
    }

    pub(crate) fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.inner.children.iter().any(|child| child == *actor)
    }

    pub(crate) async fn stop(&self, actor: &BasicActorRef) {
        actor.sys_tell(SystemCmd::Stop.into()).await;
    }

    pub fn add_child(&self, actor: BasicActorRef) {
        self.inner.children.add(actor);
    }

    pub fn remove_child(&self, actor: &BasicActorRef) {
        self.inner.children.remove(actor)
    }

    pub async fn receive_cmd<A: Actor>(&self, cmd: SystemCmd, actor: &mut Option<A>) {
        match cmd {
            SystemCmd::Stop => self.terminate(actor).await,
            SystemCmd::Restart => self.restart().await,
        }
    }

    pub async fn terminate<A: Actor>(&self, actor: &mut Option<A>) {
        // *1. Suspend non-system mailbox messages
        // *2. Iterate all children and send Stop to each
        // *3. Wait for ActorTerminated from each child

        self.inner.is_terminating.store(true, Ordering::Relaxed);

        if !self.has_children() {
            self.kernel().terminate(&self.inner.system);
            post_stop(actor).await;
        } else {
            for child in self.inner.children.iter() {
                self.stop(&child).await;
            }
        }
    }

    pub async fn restart(&self) {
        if !self.has_children() {
            self.kernel().restart(&self.inner.system);
        } else {
            self.inner.is_restarting.store(true, Ordering::Relaxed);
            for child in self.inner.children.iter() {
                self.stop(&child).await;
            }
        }
    }

    pub async fn death_watch<A: Actor>(&self, terminated: &BasicActorRef, actor: &mut Option<A>) {
        if self.is_child(terminated) {
            self.remove_child(terminated);

            if !self.has_children() {
                // No children exist. Stop this actor's kernel.
                if self.inner.is_terminating.load(Ordering::Relaxed) {
                    self.kernel().terminate(&self.inner.system);
                    post_stop(actor).await;
                }

                // No children exist. Restart the actor.
                if self.inner.is_restarting.load(Ordering::Relaxed) {
                    self.inner.is_restarting.store(false, Ordering::Relaxed);
                    self.kernel().restart(&self.inner.system);
                }
            }
        }
    }

    pub async fn handle_failure(&self, failed: BasicActorRef, strategy: Strategy) {
        match strategy {
            Strategy::Stop => self.stop(&failed).await,
            Strategy::Restart => self.restart_child(&failed).await,
            Strategy::Escalate => self.escalate_failure().await,
        }
    }

    pub async fn restart_child(&self, actor: &BasicActorRef) {
        actor.sys_tell(SystemCmd::Restart.into()).await;
    }

    pub async fn escalate_failure(&self) {
        self.inner
            .parent
            .as_ref()
            .unwrap()
            .sys_tell(SystemMsg::Failed(self.myself()))
            .await;
    }
}

impl<Msg: Message> From<ExtendedCell<Msg>> for ActorCell {
    fn from(cell: ExtendedCell<Msg>) -> Self {
        cell.cell
    }
}

impl fmt::Debug for ActorCell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ActorCell[{:?}]", self.uri())
    }
}

#[derive(Clone)]
pub struct ExtendedCell<Msg: Message> {
    cell: ActorCell,
    mailbox: MailboxSender<Msg>,
}

impl<Msg> ExtendedCell<Msg>
where
    Msg: Message,
{
    pub(crate) fn new(
        uri: ActorUri,
        parent: Option<BasicActorRef>,
        system: &ActorSystem,
        any_mailbox: Arc<dyn AnySender>,
        sys_mailbox: MailboxSender<SystemMsg>,
        mailbox: MailboxSender<Msg>,
    ) -> Self {
        let cell = ActorCell {
            inner: Arc::new(ActorCellInner {
                uri,
                parent,
                children: Children::new(),
                // is_remote: false,
                is_terminating: Arc::new(AtomicBool::new(false)),
                is_restarting: Arc::new(AtomicBool::new(false)),
                // status: Arc::new(AtomicUsize::new(0)),
                kernel: None,
                system: system.clone(),
                mailbox: any_mailbox,
                sys_mailbox,
            }),
        };

        ExtendedCell { cell, mailbox }
    }

    pub(crate) fn init(self, kernel: &KernelRef) -> Self {
        let cell = self.cell.init(kernel);

        ExtendedCell { cell, ..self }
    }

    pub fn myself(&self) -> ActorRef<Msg> {
        self.cell.myself().typed(self.clone())
    }

    pub fn uri(&self) -> &ActorUri {
        self.cell.uri()
    }

    pub fn parent(&self) -> BasicActorRef {
        self.cell.parent()
    }

    pub fn has_children(&self) -> bool {
        self.cell.has_children()
    }

    pub(crate) fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.cell.is_child(actor)
    }

    pub fn children(&self) -> &Children {
        self.cell.children()
    }

    pub fn user_root(&self) -> BasicActorRef {
        self.cell.user_root()
    }

    pub fn is_root(&self) -> bool {
        self.cell.is_root()
    }

    pub fn is_user(&self) -> bool {
        self.cell.is_user()
    }

    pub(crate) async fn send_msg(&self, msg: Envelope<Msg>) -> MsgResult<Envelope<Msg>> {
        let mb = &self.mailbox;
        let k = self.cell.kernel();

        match dispatch(msg, mb, k, self.system()).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let dl = e.clone(); // clone the failed message and send to dead letters
                let dl = DeadLetter {
                    msg: format!("{:?}", dl.msg.msg),
                    send_out: dl.msg.send_out,
                    recipient: self.cell.myself(),
                };

                self.cell
                    .inner
                    .system
                    .dead_letters()
                    .tell(
                        Publish {
                            topic: "dead_letter".into(),
                            msg: dl,
                        },
                        None,
                    )
                    .await;

                Err(e)
            }
        }
    }

    pub(crate) async fn send_sys_msg(
        &self,
        msg: Envelope<SystemMsg>,
    ) -> MsgResult<Envelope<SystemMsg>> {
        self.cell.send_sys_msg(msg).await
    }

    pub fn system(&self) -> &ActorSystem {
        &self.cell.inner.system
    }

    pub(crate) async fn handle_failure(&self, failed: BasicActorRef, strategy: Strategy) {
        self.cell.handle_failure(failed, strategy).await
    }

    pub(crate) async fn receive_cmd<A: Actor>(&self, cmd: SystemCmd, actor: &mut Option<A>) {
        self.cell.receive_cmd(cmd, actor).await
    }

    pub(crate) async fn death_watch<A: Actor>(
        &self,
        terminated: &BasicActorRef,
        actor: &mut Option<A>,
    ) {
        self.cell.death_watch(terminated, actor).await
    }
}

impl<Msg: Message> fmt::Debug for ExtendedCell<Msg> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExtendedCell[{:?}]", self.uri())
    }
}

async fn post_stop<A: Actor>(actor: &mut Option<A>) {
    // If the actor instance exists we can execute post_stop.
    // The instance will be None if this is an actor that has failed
    // and is being terminated by an escalated supervisor.
    if let Some(act) = actor.as_mut() {
        act.post_stop().await;
    }
}

/// Provides context, including the actor system during actor execution.
///
/// `Context` is passed to an actor's functions, such as
/// `receive`.
///
/// Operations performed are in most cases done so from the
/// actor's perspective. For example, creating a child actor
/// using `ctx.actor_of` will create the child under the current
/// actor within the heirarchy.
///
/// Since `Context` is specific to an actor and its functions
/// it is not cloneable.
pub struct Context<Msg: Message> {
    myself: ActorRef<Msg>,
    system: ActorSystem,
    pub(crate) kernel: KernelRef,
}

impl<Msg> Context<Msg>
where
    Msg: Message,
{
    pub fn new(myself: ActorRef<Msg>, system: ActorSystem, kernel: KernelRef) -> Self {
        Self {
            myself,
            system,
            kernel,
        }
    }

    /// Returns the `ActorRef` of the current actor.
    pub fn myself(&self) -> &ActorRef<Msg> {
        &self.myself
    }

    pub fn system(&self) -> &ActorSystem {
        &self.system
    }
}

#[async_trait::async_trait]
impl<Msg: Message> ActorRefFactory for Context<Msg> {
    async fn actor_of_props<A>(
        &self,
        name: &str,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor,
    {
        self.system
            .provider
            .create_actor(props, name, &self.myself().clone().into(), &self.system)
            .await
    }

    async fn actor_of<A>(&self, name: &str) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory,
    {
        self.system
            .provider
            .create_actor(
                Props::new::<A>(),
                name,
                &self.myself().clone().into(),
                self.system(),
            )
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
        self.system
            .provider
            .create_actor(
                Props::new_args::<A, _>(args),
                name,
                &self.myself().clone().into(),
                self.system(),
            )
            .await
    }

    async fn stop(&self, actor: impl SysTell) {
        actor.sys_tell(SystemCmd::Stop.into()).await;
    }
}

impl<Msg> ActorSelectionFactory for Context<Msg>
where
    Msg: Message,
{
    fn select(&self, path: &str) -> Result<ActorSelection, InvalidPath> {
        let (anchor, path_str) = if path.starts_with('/') {
            let anchor = self.system.user_root().clone();
            let anchor_path = format!("{}/", anchor.path());
            let path = path.to_string().replace(&anchor_path, "");

            (anchor, path)
        } else {
            (self.myself.clone().into(), path.to_string())
        };

        ActorSelection::new(
            anchor, // self.system.dead_letters(),
            path_str,
        )
    }
}

impl<Msg> RefSelectionFactory for Context<Msg>
where
    Msg: Message,
{
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

        find_actor_by_path_recursive(self.system.user_root(), path)
    }
}

impl<Msg> Run for Context<Msg>
where
    Msg: Message,
{
    fn run<Fut>(&self, future: Fut) -> JoinHandle<Fut::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send,
    {
        self.system.run(future)
    }
}

#[async_trait::async_trait]
impl<Msg> Timer for Context<Msg>
where
    Msg: Message,
{
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

        let _ = self.system.timer.send(Job::Repeat(job)).await;
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

        let _ = self.system.timer.send(Job::Once(job)).await;
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

        let _ = self.system.timer.send(Job::Once(job)).await;
        id
    }

    async fn cancel_schedule(&self, id: Uuid) {
        let _ = self.system.timer.send(Job::Cancel(id)).await;
    }
}

#[derive(Clone, Default)]
pub struct Children {
    actors: Arc<DashMap<String, BasicActorRef>>,
}

impl Children {
    pub fn new() -> Children {
        Children::default()
    }

    pub fn add(&self, actor: BasicActorRef) {
        self.actors.insert(actor.name().to_string(), actor);
    }

    pub fn remove(&self, actor: &BasicActorRef) {
        self.actors.remove(actor.name());
    }

    pub fn len(&self) -> usize {
        self.actors.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = BasicActorRef> + '_ {
        self.actors.iter().map(|e| e.value().clone())
    }
}
