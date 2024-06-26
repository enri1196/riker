use dashmap::DashMap;
use tracing::trace;

use std::sync::Arc;

use crate::{
    actor::{
        actor_cell::{ActorCell, ExtendedCell},
        *,
    },
    kernel::{kernel, mailbox::mailbox},
    system::{ActorSystem, SysActors, SystemMsg},
    validate::validate_name,
};

#[derive(Clone)]
pub struct Provider(Arc<DashMap<ActorPath, ()>>);

impl Provider {
    pub fn new() -> Self {
        Self(Arc::new(DashMap::new()))
    }

    pub async fn create_actor<A>(
        &self,
        props: BoxActorProd<A>,
        name: &str,
        parent: &BasicActorRef,
        sys: &ActorSystem,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor + 'static,
    {
        validate_name(name)?;

        let path = ActorPath::new(&format!("{}/{}", parent.path(), name));
        trace!("Attempting to create actor at: {path}");

        self.register(&path)?;

        let uri = ActorUri {
            path,
            name: Arc::from(name),
            host: sys.host(),
        };

        let (sender, sys_sender, mb) = mailbox::<A::Msg>(sys.sys_settings().msg_process_limit);

        let cell = ExtendedCell::new(
            uri,
            Some(parent.clone()),
            sys,
            // None,/*perconf*/
            Arc::new(sender.clone()),
            sys_sender,
            sender,
        );

        let k = kernel(props, cell.clone(), mb, sys).await?;
        let cell = cell.init(&k);

        let actor = ActorRef::new(cell);
        let child = BasicActorRef::from(actor.clone());
        parent.cell.add_child(child);
        actor.sys_tell(SystemMsg::ActorInit).await;

        Ok(actor)
    }

    fn register(&self, path: &ActorPath) -> Result<(), CreateError> {
        let old = self.0.insert(path.clone(), ());
        if old.is_some() {
            Err(CreateError::AlreadyExists(path.clone()))
        } else {
            Ok(())
        }
    }

    pub fn unregister(&self, path: &ActorPath) {
        self.0.remove(path);
    }
}

pub async fn create_root(sys: &ActorSystem) -> SysActors {
    let root = root(sys).await;

    SysActors {
        user: guardian("user", "/user", &root, sys).await,
        sysm: guardian("system", "/system", &root, sys).await,
        temp: guardian("temp", "/temp", &root, sys).await,
        root,
    }
}

async fn root(sys: &ActorSystem) -> BasicActorRef {
    let uri = ActorUri {
        name: Arc::from("root"),
        path: ActorPath::new("/"),
        host: Arc::from("localhost"),
    };
    let (sender, sys_sender, _mb) = mailbox::<SystemMsg>(100);

    // Big bang: all actors have a parent.
    // This means root also needs a parent.
    // An ActorCell, ActorRef and KernelRef are created
    // independently without an actor being created.
    // kernel is just a channel to nowhere
    // let (mut tx, mut _rx) = channel::<KernelMsg>(1000);
    // let bb_k = KernelRef {
    //     tx
    // };

    let bb_cell = ActorCell::new(
        uri.clone(),
        None,
        sys,
        // None, // old perfaconf
        Arc::new(sender),
        sys_sender,
    );

    let bigbang = BasicActorRef::new(bb_cell);

    // root
    let props: BoxActorProd<Guardian> = Props::new_args::<Guardian, _>("root".to_string());
    let (sender, sys_sender, mb) = mailbox::<SystemMsg>(100);

    let cell = ExtendedCell::new(
        uri,
        Some(bigbang),
        sys,
        // None,/*perconf*/
        Arc::new(sender.clone()),
        sys_sender,
        sender,
    );

    let k = kernel(props, cell.clone(), mb, sys).await.unwrap();
    let cell = cell.init(&k);
    let actor_ref = ActorRef::new(cell);

    BasicActorRef::from(actor_ref)
}

async fn guardian(
    name: &str,
    path: &str,
    root: &BasicActorRef,
    sys: &ActorSystem,
) -> BasicActorRef {
    let uri = ActorUri {
        name: Arc::from(name),
        path: ActorPath::new(path),
        host: Arc::from("localhost"),
    };

    let props: BoxActorProd<Guardian> = Props::new_args::<Guardian, _>(name.to_string());
    let (sender, sys_sender, mb) = mailbox::<SystemMsg>(100);

    let cell = ExtendedCell::new(
        uri,
        Some(root.clone()),
        sys,
        // None,/*perconf*/
        Arc::new(sender.clone()),
        sys_sender,
        sender,
    );

    let k = kernel(props, cell.clone(), mb, sys).await.unwrap();
    let cell = cell.init(&k);
    let actor_ref = ActorRef::new(cell);

    let actor = BasicActorRef::from(actor_ref);
    root.cell.add_child(actor.clone());
    actor
}

struct Guardian {
    name: String,
}

impl ActorFactoryArgs<String> for Guardian {
    fn create_args(name: String) -> Self {
        Guardian { name }
    }
}

#[async_trait::async_trait]
impl Actor for Guardian {
    type Msg = SystemMsg;

    async fn recv(&mut self, _: &Context<Self::Msg>, _: Self::Msg, _: Option<BasicActorRef>) {}

    async fn post_stop(&mut self) {
        trace!("{} guardian stopped", self.name);
    }
}
