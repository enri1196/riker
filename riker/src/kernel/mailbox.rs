use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use thiserror::Error;

use crate::{
    actor::actor_cell::ExtendedCell,
    actor::*,
    kernel::{
        queue::{queue, EnqueueResult, QueueEmpty, QueueReader, QueueWriter},
        Dock,
    },
    system::ActorCreated,
    system::{ActorSystem, SystemEvent, SystemMsg},
    AnyMessage, Envelope, Message,
};

pub trait MailboxSchedule {
    fn set_scheduled(&self, b: bool);

    fn is_scheduled(&self) -> bool;
}

#[derive(Debug, Clone, Copy, Error)]
#[error("AnyEnqueueError")]
pub struct AnyEnqueueError;

impl From<()> for AnyEnqueueError {
    fn from(_: ()) -> AnyEnqueueError {
        AnyEnqueueError
    }
}

#[async_trait::async_trait]
pub trait AnySender: Send + Sync {
    async fn try_any_enqueue(
        &self,
        msg: &mut AnyMessage,
        send_out: Option<BasicActorRef>,
    ) -> Result<(), AnyEnqueueError>;

    fn set_sched(&self, b: bool);

    fn is_sched(&self) -> bool;
}

#[derive(Clone)]
pub struct MailboxSender<Msg: Message> {
    queue: QueueWriter<Msg>,
    scheduled: Arc<AtomicBool>,
}

impl<Msg> MailboxSender<Msg>
where
    Msg: Message,
{
    pub async fn try_enqueue(&self, msg: Envelope<Msg>) -> EnqueueResult<Msg> {
        self.queue.try_enqueue(msg).await
    }
}

impl<Msg> MailboxSchedule for MailboxSender<Msg>
where
    Msg: Message,
{
    fn set_scheduled(&self, b: bool) {
        self.scheduled.store(b, Ordering::Relaxed);
    }

    fn is_scheduled(&self) -> bool {
        self.scheduled.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl<Msg> AnySender for MailboxSender<Msg>
where
    Msg: Message,
{
    async fn try_any_enqueue(
        &self,
        msg: &mut AnyMessage,
        send_out: Option<BasicActorRef>,
    ) -> Result<(), AnyEnqueueError> {
        let actual = msg.take().map_err(|_| AnyEnqueueError)?;
        let msg = Envelope {
            msg: actual,
            send_out,
        };
        self.try_enqueue(msg).await.map_err(|_| AnyEnqueueError)
    }

    fn set_sched(&self, b: bool) {
        self.set_scheduled(b)
    }

    fn is_sched(&self) -> bool {
        self.is_scheduled()
    }
}

unsafe impl<Msg: Message> Send for MailboxSender<Msg> {}
unsafe impl<Msg: Message> Sync for MailboxSender<Msg> {}

#[derive(Clone)]
pub struct Mailbox<Msg: Message> {
    inner: Arc<MailboxInner<Msg>>,
}

pub struct MailboxInner<Msg: Message> {
    msg_process_limit: u32,
    queue: QueueReader<Msg>,
    sys_queue: QueueReader<SystemMsg>,
    suspended: Arc<AtomicBool>,
    scheduled: Arc<AtomicBool>,
}

impl<Msg: Message> Mailbox<Msg> {
    pub async fn dequeue(&self) -> Envelope<Msg> {
        self.inner.queue.dequeue().await
    }

    pub async fn try_dequeue(&self) -> Result<Envelope<Msg>, QueueEmpty> {
        self.inner.queue.try_dequeue().await
    }

    pub async fn sys_try_dequeue(&self) -> Result<Envelope<SystemMsg>, QueueEmpty> {
        self.inner.sys_queue.try_dequeue().await
    }

    pub async fn has_msgs(&self) -> bool {
        self.inner.queue.has_msgs().await
    }

    pub async fn has_sys_msgs(&self) -> bool {
        self.inner.sys_queue.has_msgs().await
    }

    pub fn set_suspended(&self, b: bool) {
        self.inner.suspended.store(b, Ordering::Relaxed);
    }

    fn is_suspended(&self) -> bool {
        self.inner.suspended.load(Ordering::Relaxed)
    }

    fn msg_process_limit(&self) -> u32 {
        self.inner.msg_process_limit
    }
}

impl<Msg> MailboxSchedule for Mailbox<Msg>
where
    Msg: Message,
{
    fn set_scheduled(&self, b: bool) {
        self.inner.scheduled.store(b, Ordering::Relaxed);
    }

    fn is_scheduled(&self) -> bool {
        self.inner.scheduled.load(Ordering::Relaxed)
    }
}

pub fn mailbox<Msg>(
    msg_process_limit: u32,
) -> (MailboxSender<Msg>, MailboxSender<SystemMsg>, Mailbox<Msg>)
where
    Msg: Message,
{
    let (qw, qr) = queue::<Msg>();
    let (sqw, sqr) = queue::<SystemMsg>();

    let scheduled = Arc::new(AtomicBool::new(false));

    let sender = MailboxSender {
        queue: qw,
        scheduled: scheduled.clone(),
    };

    let sys_sender = MailboxSender {
        queue: sqw,
        scheduled: scheduled.clone(),
    };

    let mailbox = Mailbox {
        inner: Arc::new(MailboxInner {
            msg_process_limit,
            queue: qr,
            sys_queue: sqr,
            suspended: Arc::new(AtomicBool::new(true)),
            scheduled,
        }),
    };

    (sender, sys_sender, mailbox)
}

pub async fn run_mailbox<A>(mbox: &Mailbox<A::Msg>, ctx: Context<A::Msg>, dock: &mut Dock<A>)
where
    A: Actor,
{
    let sen = Sentinel {
        actor: ctx.myself().clone().into(),
        parent: ctx.myself().parent(),
        mbox,
    };

    let mut actor = dock.actor.lock().await.take();
    let cell = &mut dock.cell;

    process_sys_msgs(sen.mbox, &ctx, cell, &mut actor).await;

    if actor.is_some() && !sen.mbox.is_suspended() {
        process_msgs(sen.mbox, &ctx, cell, &mut actor).await;
    }

    process_sys_msgs(sen.mbox, &ctx, cell, &mut actor).await;

    if actor.is_some() {
        let mut a = dock.actor.lock().await;
        *a = actor;
    }

    sen.mbox.set_scheduled(false);

    let has_msgs = sen.mbox.has_msgs().await || sen.mbox.has_sys_msgs().await;
    if has_msgs && !sen.mbox.is_scheduled() {
        ctx.kernel.schedule(ctx.system());
    }
}

async fn process_msgs<A>(
    mbox: &Mailbox<A::Msg>,
    ctx: &Context<A::Msg>,
    cell: &ExtendedCell<A::Msg>,
    actor: &mut Option<A>,
) where
    A: Actor,
{
    let mut count = 0;

    while count < mbox.msg_process_limit() {
        match mbox.try_dequeue().await {
            Ok(msg) => {
                let (msg, send_out) = (msg.msg, msg.send_out);
                actor.as_mut().unwrap().recv(ctx, msg, send_out).await;
                process_sys_msgs(mbox, ctx, cell, actor).await;

                count += 1;
            }
            Err(_) => break,
        }
    }
}

async fn process_sys_msgs<A>(
    mbox: &Mailbox<A::Msg>,
    ctx: &Context<A::Msg>,
    cell: &ExtendedCell<A::Msg>,
    actor: &mut Option<A>,
) where
    A: Actor,
{
    // All system messages are processed in this mailbox execution
    // and we prevent any new messages that have since been added to the queue
    // from being processed by staging them in a Vec.
    // This prevents during actor restart.
    let mut sys_msgs: Vec<Envelope<SystemMsg>> = Vec::new();
    while let Ok(sys_msg) = mbox.sys_try_dequeue().await {
        sys_msgs.push(sys_msg);
    }

    for msg in sys_msgs {
        match msg.msg {
            SystemMsg::ActorInit => handle_init(mbox, ctx, cell, actor).await,
            SystemMsg::Command(cmd) => cell.receive_cmd(cmd, actor).await,
            SystemMsg::Event(evt) => handle_evt(evt, ctx, cell, actor).await,
            SystemMsg::Failed(failed) => handle_failed(failed, cell, actor).await,
        }
    }
}

async fn handle_init<A>(
    mbox: &Mailbox<A::Msg>,
    ctx: &Context<A::Msg>,
    cell: &ExtendedCell<A::Msg>,
    actor: &mut Option<A>,
) where
    A: Actor,
{
    actor.as_mut().unwrap().pre_start(ctx).await;
    mbox.set_suspended(false);

    if cell.is_user() {
        ctx.system()
            .publish_event(
                ActorCreated {
                    actor: cell.myself().into(),
                }
                .into(),
            )
            .await;
    }

    actor.as_mut().unwrap().post_start(ctx).await;
}

async fn handle_failed<A>(failed: BasicActorRef, cell: &ExtendedCell<A::Msg>, actor: &mut Option<A>)
where
    A: Actor,
{
    cell.handle_failure(failed, actor.as_mut().unwrap().supervisor_strategy())
        .await
}

async fn handle_evt<A>(
    evt: SystemEvent,
    ctx: &Context<A::Msg>,
    cell: &ExtendedCell<A::Msg>,
    actor: &mut Option<A>,
) where
    A: Actor,
{
    if let Some(actor) = actor {
        actor
            .sys_recv(ctx, SystemMsg::Event(evt.clone()), None)
            .await;
    }

    if let SystemEvent::ActorTerminated(terminated) = evt {
        cell.death_watch(&terminated.actor, actor).await;
    }
}

struct Sentinel<'a, Msg: Message> {
    parent: BasicActorRef,
    actor: BasicActorRef,
    mbox: &'a Mailbox<Msg>,
}

impl<'a, Msg> Drop for Sentinel<'a, Msg>
where
    Msg: Message,
{
    fn drop(&mut self) {
        if thread::panicking() {
            // Suspend the mailbox to prevent further message processing
            self.mbox.set_suspended(true);

            // There is no actor to park but kernel still needs to mark as no longer scheduled
            // self.kernel.park_actor(self.actor.uri.uid, None);
            self.mbox.set_scheduled(false);

            // Message the parent (this failed actor's supervisor) to decide how to handle the failure
            let parent = self.parent.clone();
            let actor = self.actor.clone();
            tokio::spawn(async move {
                parent.sys_tell(SystemMsg::Failed(actor.clone())).await;
            });
        }
    }
}

pub async fn flush_to_deadletters<Msg>(
    mbox: &Mailbox<Msg>,
    actor: &BasicActorRef,
    sys: &ActorSystem,
) where
    Msg: Message,
{
    while let Ok(Envelope { msg, send_out }) = mbox.try_dequeue().await {
        let dl = DeadLetter {
            msg: format!("{:?}", msg),
            send_out,
            recipient: actor.clone(),
        };

        sys.dead_letters()
            .tell(
                Publish {
                    topic: "dead_letter".into(),
                    msg: dl,
                },
                None,
            )
            .await;
    }
}

// #[derive(Clone, Debug)]
// pub struct MailboxConfig {
//     pub msg_process_limit: u32,
// }

// impl<'a> From<&'a Config> for MailboxConfig {
//     fn from(cfg: &Config) -> Self {
//         MailboxConfig {
//             msg_process_limit: cfg.get_int("mailbox.msg_process_limit").unwrap() as u32,
//         }
//     }
// }
