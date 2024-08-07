use crate::system::SystemEvent;

/// Topics allow channel subscribers to filter messages by interest
///
/// When publishing a message to a channel a Topic is provided.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Topic(Box<str>);

impl From<&str> for Topic {
    fn from(topic: &str) -> Self {
        Topic(topic.into())
    }
}

impl From<String> for Topic {
    fn from(topic: String) -> Self {
        Topic(topic.into())
    }
}

impl From<&SystemEvent> for Topic {
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
