use std::{
    fmt::{self, Debug},
    ops::Deref,
    sync::Arc,
};

use crate::{
    actor::{
        actor_cell::{ActorCell, ExtendedCell},
        actor_traits::*,
        ActorPath, ActorUri,
    },
    kernel::mailbox::AnyEnqueueError,
    system::{ActorSystem, SystemMsg},
    AnyMessage, Envelope, Message,
};

/// A lightweight, typed reference to interact with its underlying
/// actor instance through concurrent messaging.
///
/// All ActorRefs are products of `system.actor_of`
/// or `context.actor_of`. When an actor is created using `actor_of`
/// an `ActorRef<Msg>` is returned, where `Msg` is the mailbox
/// message type for the actor.
///
/// Actor references are lightweight and can be cloned without concern
/// for memory use.
///
/// Messages sent to an actor are added to the actor's mailbox.
///
/// In the event that the underlying actor is terminated messages sent
/// to the actor will be routed to dead letters.
///
/// If an actor is restarted all existing references continue to
/// be valid.
#[derive(Clone)]
pub struct ActorRef<Msg: Message> {
    pub cell: ExtendedCell<Msg>,
}

impl<Msg: Message> ActorRef<Msg> {
    #[doc(hidden)]
    pub fn new(cell: ExtendedCell<Msg>) -> ActorRef<Msg> {
        ActorRef { cell }
    }

    pub async fn send_msg(&self, msg: Msg, send_out: impl Into<Option<BasicActorRef>>) {
        let envelope = Envelope {
            msg,
            send_out: send_out.into(),
        };
        // consume the result (we don't return it to user)
        let _ = self.cell.send_msg(envelope).await;
    }
}

#[async_trait::async_trait]
impl<T, M> Tell<T> for ActorRef<M>
where
    T: Message + Into<M>,
    M: Message,
{
    async fn tell(&self, msg: T, send_out: Option<BasicActorRef>) {
        self.send_msg(msg.into(), send_out).await;
    }

    fn box_clone(&self) -> BoxedTell<T> {
        BoxedTell(Arc::new(self.clone()))
    }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for ActorRef<M> {
    async fn sys_tell(&self, msg: SystemMsg) {
        let envelope = Envelope {
            send_out: None,
            msg,
        };
        let _ = self.cell.send_sys_msg(envelope).await;
    }
}

impl<Msg: Message> ActorReference for ActorRef<Msg> {
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str {
        self.cell.uri().name.as_ref()
    }

    fn uri(&self) -> &ActorUri {
        self.cell.uri()
    }

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b`
    fn path(&self) -> &ActorPath {
        &self.cell.uri().path
    }

    fn is_root(&self) -> bool {
        self.cell.is_root()
    }

    /// Parent reference.
    fn parent(&self) -> BasicActorRef {
        self.cell.parent()
    }

    fn user_root(&self) -> BasicActorRef {
        self.cell.user_root()
    }

    fn has_children(&self) -> bool {
        self.cell.has_children()
    }

    fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.cell.is_child(actor)
    }

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a> {
        self.cell.children()
    }

    // fn sys_tell(&self, msg: SystemMsg) {
    //     let envelope = Envelope { msg, sender: None };
    //     let _ = self.cell.send_sys_msg(envelope);
    // }
}

impl<Msg: Message> ActorReference for &ActorRef<Msg> {
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str {
        self.cell.uri().name.as_ref()
    }

    fn uri(&self) -> &ActorUri {
        self.cell.uri()
    }

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b`
    fn path(&self) -> &ActorPath {
        &self.cell.uri().path
    }

    fn is_root(&self) -> bool {
        self.cell.is_root()
    }

    /// Parent reference.
    fn parent(&self) -> BasicActorRef {
        self.cell.parent()
    }

    fn user_root(&self) -> BasicActorRef {
        self.cell.user_root()
    }

    fn has_children(&self) -> bool {
        self.cell.has_children()
    }

    fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.cell.is_child(actor)
    }

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a> {
        self.cell.children()
    }
}

#[async_trait::async_trait]
impl<M: Message> SysTell for &ActorRef<M> {
    async fn sys_tell(&self, msg: SystemMsg) {
        let envelope = Envelope {
            msg,
            send_out: None,
        };
        let _ = self.cell.send_sys_msg(envelope).await;
    }
}

impl<Msg: Message> fmt::Debug for ActorRef<Msg> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ActorRef[{:?}]", self.uri())
    }
}

impl<Msg: Message> fmt::Display for ActorRef<Msg> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ActorRef[{}]", self.uri())
    }
}

impl<Msg: Message> PartialEq for ActorRef<Msg> {
    fn eq(&self, other: &ActorRef<Msg>) -> bool {
        self.uri().path == other.uri().path
    }
}

pub struct BoxedTell<T>(pub Arc<dyn Tell<T> + Send + Sync + 'static>);

impl<T> ActorReference for BoxedTell<T>
where
    T: Message,
{
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str {
        self.0.name()
    }

    fn uri(&self) -> &ActorUri {
        self.0.uri()
    }

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b
    fn path(&self) -> &ActorPath {
        self.0.path()
    }

    fn is_root(&self) -> bool {
        self.0.is_root()
    }

    /// Parent reference.
    fn parent(&self) -> BasicActorRef {
        self.0.parent()
    }

    fn user_root(&self) -> BasicActorRef {
        self.0.user_root()
    }

    fn has_children(&self) -> bool {
        self.0.has_children()
    }

    fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.0.is_child(actor)
    }

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a> {
        self.0.children()
    }
}

#[async_trait::async_trait]
impl<T: Debug + Clone + Send + 'static> SysTell for BoxedTell<T> {
    async fn sys_tell(&self, msg: SystemMsg) {
        self.0.sys_tell(msg).await
    }
}

impl<T> Deref for BoxedTell<T> {
    type Target = Arc<dyn Tell<T> + Send + Sync + 'static>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> PartialEq for BoxedTell<T> {
    fn eq(&self, other: &BoxedTell<T>) -> bool {
        self.path() == other.path()
    }
}

impl<T> fmt::Debug for BoxedTell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Tell[{:?}]", self.uri())
    }
}

impl<T> fmt::Display for BoxedTell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Tell[{}]", self.uri())
    }
}

impl<T: 'static> Clone for BoxedTell<T> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

/// A lightweight, un-typed reference to interact with its underlying
/// actor instance through concurrent messaging.
///
/// `BasicActorRef` can be derived from an original `ActorRef<Msg>`.
///
/// `BasicActorRef` allows for un-typed messaging using `try_tell`,
/// that will return a `Result`. If the message type was not supported,
/// the result will contain an `Error`.
///
/// `BasicActorRef` can be used when the original `ActorRef` isn't available,
/// when you need to use collections to store references from different actor
/// types, or when using actor selections to message parts of the actor hierarchy.
///
/// In general, it is better to use `ActorRef` where possible.
#[derive(Clone)]
pub struct BasicActorRef {
    pub cell: ActorCell,
}

impl BasicActorRef {
    #[doc(hidden)]
    pub fn new(cell: ActorCell) -> BasicActorRef {
        BasicActorRef { cell }
    }

    pub fn typed<Msg: Message>(&self, cell: ExtendedCell<Msg>) -> ActorRef<Msg> {
        ActorRef { cell }
    }

    pub(crate) fn sys_init(&self, sys: &ActorSystem) {
        self.cell.kernel().sys_init(sys);
    }

    /// Send a message to this actor
    ///
    /// Returns a result. If the message type is not supported Error is returned.
    pub async fn try_tell<Msg>(
        &self,
        msg: Msg,
        sender: impl Into<Option<BasicActorRef>>,
    ) -> Result<(), AnyEnqueueError>
    where
        Msg: Message + Send,
    {
        self.try_tell_any(&mut AnyMessage::new(msg, true), sender)
            .await
    }

    pub async fn try_tell_any(
        &self,
        msg: &mut AnyMessage,
        sender: impl Into<Option<BasicActorRef>>,
    ) -> Result<(), AnyEnqueueError> {
        self.cell.send_any_msg(msg, sender.into()).await
    }
}

impl ActorReference for BasicActorRef {
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str {
        self.cell.uri().name.as_ref()
    }

    fn uri(&self) -> &ActorUri {
        self.cell.uri()
    }

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b`
    fn path(&self) -> &ActorPath {
        &self.cell.uri().path
    }

    fn is_root(&self) -> bool {
        self.cell.is_root()
    }

    /// Parent reference.
    fn parent(&self) -> BasicActorRef {
        self.cell.parent()
    }

    fn user_root(&self) -> BasicActorRef {
        self.cell.user_root()
    }

    fn has_children(&self) -> bool {
        self.cell.has_children()
    }

    fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.cell.is_child(actor)
    }

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a> {
        self.cell.children()
    }
}

#[async_trait::async_trait]
impl SysTell for BasicActorRef {
    async fn sys_tell(&self, msg: SystemMsg) {
        let envelope = Envelope {
            msg,
            send_out: None,
        };
        let _ = self.cell.send_sys_msg(envelope).await;
    }
}

impl ActorReference for &BasicActorRef {
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str {
        self.cell.uri().name.as_ref()
    }

    fn uri(&self) -> &ActorUri {
        self.cell.uri()
    }

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b`
    fn path(&self) -> &ActorPath {
        &self.cell.uri().path
    }

    fn is_root(&self) -> bool {
        self.cell.is_root()
    }

    /// Parent reference.
    fn parent(&self) -> BasicActorRef {
        self.cell.parent()
    }

    fn user_root(&self) -> BasicActorRef {
        self.cell.user_root()
    }

    fn has_children(&self) -> bool {
        self.cell.has_children()
    }

    fn is_child(&self, actor: &BasicActorRef) -> bool {
        self.cell.is_child(actor)
    }

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a> {
        self.cell.children()
    }
}

#[async_trait::async_trait]
impl SysTell for &BasicActorRef {
    async fn sys_tell(&self, msg: SystemMsg) {
        let envelope = Envelope {
            msg,
            send_out: None,
        };
        let _ = self.cell.send_sys_msg(envelope).await;
    }
}

impl fmt::Debug for BasicActorRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BasicActorRef[{:?}]", self.cell.uri())
    }
}

impl fmt::Display for BasicActorRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BasicActorRef[{}]", self.cell.uri())
    }
}

impl PartialEq for BasicActorRef {
    fn eq(&self, other: &BasicActorRef) -> bool {
        self.cell.uri().path == other.cell.uri().path
    }
}

impl<Msg> From<ActorRef<Msg>> for BasicActorRef
where
    Msg: Message,
{
    fn from(actor: ActorRef<Msg>) -> BasicActorRef {
        BasicActorRef::new(ActorCell::from(actor.cell))
    }
}

impl<Msg> From<ActorRef<Msg>> for Option<BasicActorRef>
where
    Msg: Message,
{
    fn from(actor: ActorRef<Msg>) -> Option<BasicActorRef> {
        Some(BasicActorRef::new(ActorCell::from(actor.cell)))
    }
}
