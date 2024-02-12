pub(crate) mod kernel_ref;
pub(crate) mod mailbox;
pub(crate) mod provider;
pub(crate) mod queue;

use crate::{actors::Run, system::ActorSystem};

#[derive(Debug)]
pub enum KernelMsg {
    TerminateActor,
    RestartActor,
    RunActor,
    Sys(ActorSystem),
}
use std::{panic::AssertUnwindSafe, sync::Arc};
use tokio::sync::Mutex;

use futures::FutureExt;
use tokio::sync::mpsc::channel;
use tracing::warn;

use crate::{
    actor::actor_cell::ExtendedCell,
    actor::*,
    kernel::{
        kernel_ref::KernelRef,
        mailbox::{flush_to_deadletters, run_mailbox, Mailbox},
    },
    system::{ActorRestarted, ActorTerminated, SystemMsg},
    Message,
};

pub struct Dock<A: Actor> {
    pub actor: Arc<Mutex<Option<A>>>,
    pub cell: ExtendedCell<A::Msg>,
}

impl<A: Actor> Clone for Dock<A> {
    fn clone(&self) -> Dock<A> {
        Dock {
            actor: self.actor.clone(),
            cell: self.cell.clone(),
        }
    }
}

pub async fn kernel<A>(
    props: BoxActorProd<A>,
    cell: ExtendedCell<A::Msg>,
    mailbox: Mailbox<A::Msg>,
    sys: &ActorSystem,
) -> Result<KernelRef, CreateError>
where
    A: Actor + 'static,
{
    let (tx, mut rx) = channel::<KernelMsg>(1000); // todo config?
    let kr = KernelRef { tx };
    let actor = start_actor(&props).await?;

    let outer_sys = sys.clone();
    let outer_kr = kr.clone();
    let f = async move {
        let mut asys = outer_sys;
        let akr = outer_kr;
        let cell = cell.init(&akr);
        let actor = actor;

        let mut dock = Dock {
            actor: Arc::new(Mutex::new(Some(actor))),
            cell: cell.clone(),
        };

        let actor_ref = ActorRef::new(cell);

        while let Some(msg) = rx.recv().await {
            match msg {
                KernelMsg::RunActor => {
                    let ctx = Context::new(actor_ref.clone(), asys.clone(), akr.clone());

                    let _ = AssertUnwindSafe(run_mailbox(&mailbox, ctx, &mut dock))
                        .catch_unwind()
                        .await;
                }
                KernelMsg::RestartActor => {
                    restart_actor(&dock, actor_ref.clone().into(), &props, &asys).await;
                }
                KernelMsg::TerminateActor => {
                    terminate_actor(&mailbox, actor_ref.clone().into(), &asys).await;
                    break;
                }
                KernelMsg::Sys(s) => {
                    asys = s;
                }
            }
        }
    };

    sys.run(f);
    Ok(kr)
}

async fn restart_actor<A>(
    dock: &Dock<A>,
    actor_ref: BasicActorRef,
    props: &BoxActorProd<A>,
    sys: &ActorSystem,
) where
    A: Actor,
{
    let mut a = dock.actor.lock().await;
    match start_actor(props).await {
        Ok(actor) => {
            *a = Some(actor);
            actor_ref.sys_tell(SystemMsg::ActorInit).await;
            sys.publish_event(ActorRestarted { actor: actor_ref }.into())
                .await;
        }
        Err(_) => {
            warn!("Actor failed to restart: {:?}", actor_ref);
        }
    }
}

async fn terminate_actor<Msg>(mbox: &Mailbox<Msg>, actor_ref: BasicActorRef, sys: &ActorSystem)
where
    Msg: Message,
{
    sys.provider.unregister(actor_ref.path());
    flush_to_deadletters(mbox, &actor_ref, sys).await;
    sys.publish_event(
        ActorTerminated {
            actor: actor_ref.clone(),
        }
        .into(),
    )
    .await;

    let parent = actor_ref.parent();
    if !parent.is_root() {
        parent
            .sys_tell(ActorTerminated { actor: actor_ref }.into())
            .await;
    }
}

async fn start_actor<A>(props: &BoxActorProd<A>) -> Result<A, CreateError>
where
    A: Actor,
{
    // let actor = catch_unwind(|| props.produce()).map_err(|_| CreateError::Panicked)?;
    let actor = AssertUnwindSafe(props.produce())
        .catch_unwind()
        .await
        .map_err(|_| CreateError::Panicked)?;

    Ok(actor)
}
