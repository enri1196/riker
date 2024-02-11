use crate::{actors::SystemMsg, Message};

use super::{
    actor_ref::BoxedTell, ActorArgs, ActorFactory, ActorFactoryArgs, ActorPath, ActorRef, ActorUri, BasicActorRef, BoxActorProd, Context, CreateError, Strategy
};

pub trait Actor: Send + 'static {
    type Msg: Message;

    /// Invoked when an actor is being started by the system.
    ///
    /// Any initialization inherent to the actor's role should be
    /// performed here.
    ///
    /// Panics in `pre_start` do not invoke the
    /// supervision strategy and the actor will be terminated.
    fn pre_start(&mut self, ctx: &Context<Self::Msg>) {}

    /// Invoked after an actor has started.
    ///
    /// Any post initialization can be performed here, such as writing
    /// to a log file, emmitting metrics.
    ///
    /// Panics in `post_start` follow the supervision strategy.
    fn post_start(&mut self, ctx: &Context<Self::Msg>) {}

    /// Invoked after an actor has been stopped.
    fn post_stop(&mut self) {}

    /// Return a supervisor strategy that will be used when handling failed child actors.
    fn supervisor_strategy(&self) -> Strategy {
        Strategy::Restart
    }

    /// Invoked when an actor receives a system message
    ///
    /// It is guaranteed that only one message in the actor's mailbox is processed
    /// at any one time, including `recv` and `sys_recv`.
    fn sys_recv(
        &mut self,
        ctx: &Context<Self::Msg>,
        msg: SystemMsg,
        send_out: Option<BasicActorRef>,
    ) {
    }

    /// Invoked when an actor receives a message
    ///
    /// It is guaranteed that only one message in the actor's mailbox is processed
    /// at any one time, including `recv` and `sys_recv`.
    fn recv(&mut self, ctx: &Context<Self::Msg>, msg: Self::Msg, send_out: Option<BasicActorRef>);
}

pub trait ActorReference {
    /// Actor name.
    ///
    /// Unique among siblings.
    fn name(&self) -> &str;

    /// Actor URI.
    ///
    /// Returns the URI for this actor.
    fn uri(&self) -> &ActorUri;

    /// Actor path.
    ///
    /// e.g. `/user/actor_a/actor_b`
    fn path(&self) -> &ActorPath;

    /// True if this actor is the top level root
    ///
    /// I.e. `/root`
    fn is_root(&self) -> bool;

    /// User root reference
    ///
    /// I.e. `/root/user`
    fn user_root(&self) -> BasicActorRef;

    /// Parent reference
    ///
    /// Returns the `BasicActorRef` of this actor's parent actor
    fn parent(&self) -> BasicActorRef;

    /// True is this actor has any children actors
    fn has_children(&self) -> bool;

    /// True if the given actor is a child of this actor
    fn is_child(&self, actor: &BasicActorRef) -> bool;

    /// Iterator over children references.
    fn children<'a>(&'a self) -> Box<dyn Iterator<Item = BasicActorRef> + 'a>;
}

pub trait Tell<T>: ActorReference + SysTell + Send + 'static {
    fn tell(&self, msg: T, send_out: Option<BasicActorRef>);
    /// Send a system message to this actor
    fn box_clone(&self) -> BoxedTell<T>;
}

pub trait SysTell: ActorReference + Send {
    fn sys_tell(&self, msg: SystemMsg);
}

/// Receive and handle a specific message type
///
/// This trait is typically used in conjuction with the #[actor]
/// attribute macro and implemented for each message type to receive.
///
/// # Examples
///
/// ```
/// # use riker::actors::*;
///
/// #[derive(Clone, Debug)]
/// pub struct Foo;
/// #[derive(Clone, Debug)]
/// pub struct Bar;
/// #[actor(Foo, Bar)] // <-- set our actor to receive Foo and Bar types
/// #[derive(Default)]
/// struct MyActor;
///
/// impl Actor for MyActor {
///     type Msg = MyActorMsg; // <-- MyActorMsg is provided for us
///
///     fn recv(&mut self,
///                 ctx: &Context<Self::Msg>,
///                 msg: Self::Msg,
///                 send_out: Option<BasicActorRef>) {
///         self.receive(ctx, msg, send_out); // <-- call the respective implementation
///     }
/// }
///
/// impl Receive<Foo> for MyActor {
///     fn receive(&mut self,
///                 ctx: &Context<Self::Msg>,
///                 msg: Foo, // <-- receive Foo
///                 send_out: Option<BasicActorRef>) {
///         println!("Received a Foo");
///     }
/// }
///
/// impl Receive<Bar> for MyActor {
///     fn receive(&mut self,
///                 ctx: &Context<Self::Msg>,
///                 msg: Bar, // <-- receive Bar
///                 send_out: Option<BasicActorRef>) {
///         println!("Received a Bar");
///     }
/// }
///
/// // main
/// # tokio_test::block_on(async {
/// let sys = ActorSystem::new().unwrap();
/// let actor = sys.actor_of::<MyActor>("my-actor").unwrap();
///
/// actor.tell(Foo, None);
/// actor.tell(Bar, None);
/// })
/// ```
pub trait Receive<Msg: Message>: Actor {
    /// Invoked when an actor receives a message
    ///
    /// It is guaranteed that only one message in the actor's mailbox is processed
    /// at any one time, including `receive`, `other_receive` and `system_receive`.
    fn receive(&mut self, ctx: &Context<Self::Msg>, msg: Msg, send_out: Option<BasicActorRef>);
}

/// Produces `ActorRef`s. `actor_of` blocks on the current thread until
/// the actor has successfully started or failed to start.
///
/// It is advised to return from the actor's factory method quickly and
/// handle any initialization in the actor's `pre_start` method, which is
/// invoked after the `ActorRef` is returned.
pub trait ActorRefFactory {
    fn actor_of_props<A>(
        &self,
        name: &str,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor;

    fn actor_of<A>(&self, name: &str) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory + Actor;

    fn actor_of_args<A, Args>(
        &self,
        name: &str,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>;

    fn stop(&self, actor: impl SysTell);
}

/// Produces `ActorRef`s under the `temp` guardian actor.
pub trait TmpActorRefFactory {
    fn tmp_actor_of_props<A>(
        &self,
        props: BoxActorProd<A>,
    ) -> Result<ActorRef<A::Msg>, CreateError>
    where
        A: Actor;

    fn tmp_actor_of<A>(&self) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        A: ActorFactory + Actor;

    fn tmp_actor_of_args<A, Args>(
        &self,
        args: Args,
    ) -> Result<ActorRef<<A as Actor>::Msg>, CreateError>
    where
        Args: ActorArgs,
        A: ActorFactoryArgs<Args>;
}
