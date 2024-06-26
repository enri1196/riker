use std::{collections::HashMap, hash::Hash};

use crate::{
    actor::{
        Actor, ActorRef, ActorRefFactory, ActorReference, BasicActorRef, Context, CreateError,
        Receive,
    },
    system::{SystemEvent, SystemMsg},
    Message,
};

use super::actor_ref::BoxedTell;

type Subs<Msg> = HashMap<Topic, Vec<BoxedTell<Msg>>>;

/// A specialized actor for providing Publish/Subscribe capabilities to users.
///

// Generic Channel
pub type ChannelCtx<Msg> = Context<ChannelMsg<Msg>>;
pub type ChannelRef<Msg> = ActorRef<ChannelMsg<Msg>>;

/// A specialized actor for providing Publish/Subscribe capabilities for user level messages
pub struct Channel<Msg: Message> {
    subs: Subs<Msg>,
}

impl<Msg: Message> Default for Channel<Msg> {
    fn default() -> Self {
        Channel {
            subs: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl<Msg> Actor for Channel<Msg>
where
    Msg: Message,
{
    type Msg = ChannelMsg<Msg>;

    // todo subscribe to events to unsub subscribers when they die
    async fn pre_start(&mut self, _ctx: &ChannelCtx<Msg>) {
        // let sub = Subscribe {
        //     topic: SysTopic::ActorTerminated.into(),
        //     actor: Box::new(ctx.myself.clone())//.into()
        // };

        // let msg = ChannelMsg::Subscribe(sub);
        // ctx.myself.tell(msg, None);
    }

    async fn recv(
        &mut self,
        ctx: &ChannelCtx<Msg>,
        msg: ChannelMsg<Msg>,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
    }

    // We expect to receive ActorTerminated messages because we subscribed
    // to this system event. This allows us to remove actors that have been
    // terminated but did not explicity unsubscribe before terminating.
    async fn sys_recv(
        &mut self,
        _: &ChannelCtx<Msg>,
        msg: SystemMsg,
        _send_out: Option<BasicActorRef>,
    ) {
        if let SystemMsg::Event(evt) = msg {
            if let SystemEvent::ActorTerminated(terminated) = evt {
                let subs = self.subs.clone();

                for topic in subs.keys() {
                    unsubscribe(&mut self.subs, topic, &terminated.actor);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<Msg> Receive<ChannelMsg<Msg>> for Channel<Msg>
where
    Msg: Message,
{
    async fn receive(
        &mut self,
        ctx: &ChannelCtx<Msg>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        match msg {
            ChannelMsg::Publish(p) => self.receive(ctx, p, send_out).await,
            ChannelMsg::Subscribe(sub) => self.receive(ctx, sub, send_out).await,
            ChannelMsg::Unsubscribe(unsub) => self.receive(ctx, unsub, send_out).await,
            ChannelMsg::UnsubscribeAll(unsub) => self.receive(ctx, unsub, send_out).await,
        }
    }
}

#[async_trait::async_trait]
impl<Msg> Receive<Subscribe<Msg>> for Channel<Msg>
where
    Msg: Message,
{
    async fn receive(
        &mut self,
        _ctx: &ChannelCtx<Msg>,
        msg: Subscribe<Msg>,
        _send_out: Option<BasicActorRef>,
    ) {
        let subs = self.subs.entry(msg.topic).or_default();
        subs.push(msg.actor);
    }
}

#[async_trait::async_trait]
impl<Msg> Receive<Unsubscribe<Msg>> for Channel<Msg>
where
    Msg: Message,
{
    async fn receive(
        &mut self,
        _ctx: &ChannelCtx<Msg>,
        msg: Unsubscribe<Msg>,
        _send_out: Option<BasicActorRef>,
    ) {
        unsubscribe(&mut self.subs, &msg.topic, &msg.actor);
    }
}

#[async_trait::async_trait]
impl<Msg> Receive<UnsubscribeAll<Msg>> for Channel<Msg>
where
    Msg: Message,
{
    async fn receive(
        &mut self,
        _ctx: &ChannelCtx<Msg>,
        msg: UnsubscribeAll<Msg>,
        _send_out: Option<BasicActorRef>,
    ) {
        let subs = self.subs.clone();

        for topic in subs.keys() {
            unsubscribe(&mut self.subs, topic, &msg.actor);
        }
    }
}

#[async_trait::async_trait]
impl<Msg> Receive<Publish<Msg>> for Channel<Msg>
where
    Msg: Message,
{
    async fn receive(
        &mut self,
        _ctx: &ChannelCtx<Msg>,
        msg: Publish<Msg>,
        send_out: Option<BasicActorRef>,
    ) {
        // send system event to actors subscribed to all topics
        if let Some(subs) = self.subs.get(&All.into()) {
            for sub in subs.iter() {
                sub.tell(msg.msg.clone(), send_out.clone()).await;
            }
        }

        // send system event to actors subscribed to the topic
        if let Some(subs) = self.subs.get(&msg.topic) {
            for sub in subs.iter() {
                sub.tell(msg.msg.clone(), send_out.clone()).await;
            }
        }
    }
}

fn unsubscribe<Msg>(subs: &mut Subs<Msg>, topic: &Topic, actor: &dyn ActorReference) {
    // Nightly only: self.subs.get(msg_type).unwrap().remove_item(actor);
    if subs.contains_key(topic) {
        if let Some(pos) = subs
            .get(topic)
            .unwrap()
            .iter()
            .position(|x| x.path() == actor.path())
        {
            subs.get_mut(topic).unwrap().remove(pos);
        }
    }
}

/// A specialized channel that publishes messages as system messages
#[derive(Default)]
pub struct EventsChannel(Channel<SystemEvent>);

#[async_trait::async_trait]
impl Actor for EventsChannel {
    type Msg = ChannelMsg<SystemEvent>;

    async fn pre_start(&mut self, ctx: &ChannelCtx<SystemEvent>) {
        self.0.pre_start(ctx).await;
    }

    async fn recv(
        &mut self,
        ctx: &ChannelCtx<SystemEvent>,
        msg: ChannelMsg<SystemEvent>,
        send_out: Option<BasicActorRef>,
    ) {
        self.receive(ctx, msg, send_out).await;
    }

    async fn sys_recv(
        &mut self,
        ctx: &ChannelCtx<SystemEvent>,
        msg: SystemMsg,
        send_out: Option<BasicActorRef>,
    ) {
        self.0.sys_recv(ctx, msg, send_out).await;
    }
}

#[async_trait::async_trait]
impl Receive<ChannelMsg<SystemEvent>> for EventsChannel {
    async fn receive(
        &mut self,
        ctx: &ChannelCtx<SystemEvent>,
        msg: Self::Msg,
        send_out: Option<BasicActorRef>,
    ) {
        // Publish variant uses specialized EventsChannel Receive
        // All other variants use the wrapped Channel (self.0) Receive(s)
        match msg {
            ChannelMsg::Publish(p) => self.receive(ctx, p, send_out).await,
            ChannelMsg::Subscribe(sub) => self.0.receive(ctx, sub, send_out).await,
            ChannelMsg::Unsubscribe(unsub) => self.0.receive(ctx, unsub, send_out).await,
            ChannelMsg::UnsubscribeAll(unsub) => self.0.receive(ctx, unsub, send_out).await,
        }
    }
}

#[async_trait::async_trait]
impl Receive<Publish<SystemEvent>> for EventsChannel {
    async fn receive(
        &mut self,
        _ctx: &ChannelCtx<SystemEvent>,
        msg: Publish<SystemEvent>,
        _send_out: Option<BasicActorRef>,
    ) {
        // send system event to actors subscribed to all topics
        if let Some(subs) = self.0.subs.get(&All.into()) {
            for sub in subs.iter() {
                let evt = SystemMsg::Event(msg.msg.clone());
                sub.sys_tell(evt).await;
            }
        }

        // send system event to actors subscribed to the topic
        if let Some(subs) = self.0.subs.get(&msg.topic) {
            for sub in subs.iter() {
                let evt = SystemMsg::Event(msg.msg.clone());
                sub.sys_tell(evt).await;
            }
        }
    }
}

// Deadletter channel implementations
pub type DLChannelMsg = ChannelMsg<DeadLetter>;

#[derive(Clone, Debug)]
pub struct DeadLetter {
    pub msg: String,
    pub send_out: Option<BasicActorRef>,
    pub recipient: BasicActorRef,
}

#[derive(Debug, Clone)]
pub struct Subscribe<Msg: Message> {
    pub topic: Topic,
    pub actor: BoxedTell<Msg>,
}

#[derive(Debug, Clone)]
pub struct Unsubscribe<Msg: Message> {
    pub topic: Topic,
    pub actor: BoxedTell<Msg>,
}

#[derive(Debug, Clone)]
pub struct UnsubscribeAll<Msg: Message> {
    pub actor: BoxedTell<Msg>,
}

#[derive(Debug, Clone)]
pub struct Publish<Msg: Message> {
    pub topic: Topic,
    pub msg: Msg,
}

#[derive(Debug, Clone)]
pub enum ChannelMsg<Msg: Message> {
    /// Publish message
    Publish(Publish<Msg>),

    /// Subscribe given `ActorRef` to a topic on a channel
    Subscribe(Subscribe<Msg>),

    /// Unsubscribe the given `ActorRef` from a topic on a channel
    Unsubscribe(Unsubscribe<Msg>),

    /// Unsubscribe the given `ActorRef` from all topics on a channel
    UnsubscribeAll(UnsubscribeAll<Msg>),
}

// publish
impl<Msg: Message> Into<ChannelMsg<Msg>> for Publish<Msg> {
    fn into(self) -> ChannelMsg<Msg> {
        ChannelMsg::Publish(self)
    }
}

// subscribe
impl<Msg: Message> Into<ChannelMsg<Msg>> for Subscribe<Msg> {
    fn into(self) -> ChannelMsg<Msg> {
        ChannelMsg::Subscribe(self)
    }
}

// unsubscribe
impl<Msg: Message> Into<ChannelMsg<Msg>> for Unsubscribe<Msg> {
    fn into(self) -> ChannelMsg<Msg> {
        ChannelMsg::Unsubscribe(self)
    }
}

// unsubscribe
impl<Msg: Message> Into<ChannelMsg<Msg>> for UnsubscribeAll<Msg> {
    fn into(self) -> ChannelMsg<Msg> {
        ChannelMsg::UnsubscribeAll(self)
    }
}

// Topics allow channel subscribers to filter messages by interest
///
/// When publishing a message to a channel a Topic is provided.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Topic(String);

impl<'a> From<&'a str> for Topic {
    fn from(topic: &str) -> Self {
        Topic(topic.to_string())
    }
}

impl From<String> for Topic {
    fn from(topic: String) -> Self {
        Topic(topic)
    }
}

impl<'a> From<&'a SystemEvent> for Topic {
    fn from(evt: &SystemEvent) -> Self {
        match *evt {
            SystemEvent::ActorCreated(_) => Topic::from("actor.created"),
            SystemEvent::ActorTerminated(_) => Topic::from("actor.terminated"),
            SystemEvent::ActorRestarted(_) => Topic::from("actor.restarted"),
        }
    }
}

/// A channel topic representing all topics `*`
pub struct All;

impl From<All> for Topic {
    fn from(_all: All) -> Self {
        Topic::from("*")
    }
}

/// System topics used by the `event_stream` channel
pub enum SysTopic {
    ActorCreated,
    ActorTerminated,
    ActorRestarted,
}

impl From<SysTopic> for Topic {
    fn from(evt: SysTopic) -> Self {
        match evt {
            SysTopic::ActorCreated => Topic::from("actor.created"),
            SysTopic::ActorTerminated => Topic::from("actor.terminated"),
            SysTopic::ActorRestarted => Topic::from("actor.restarted"),
        }
    }
}

pub async fn channel<Msg>(
    name: &str,
    fact: &impl ActorRefFactory,
) -> Result<ChannelRef<Msg>, CreateError>
where
    Msg: Message,
{
    fact.actor_of::<Channel<Msg>>(name).await
}
