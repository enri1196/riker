use std::iter::Peekable;

use crate::{
    actor::{ActorReference, BasicActorRef, SysTell},
    system::SystemMsg,
    validate::{validate_path, InvalidPath},
    Message,
};

/// A selection represents part of the actor heirarchy, allowing
/// messages to be sent to all actors in the selection.
///
/// There are several use cases where you would interact with actors
/// via a selection instead of actor references:
///
/// - You know the path of an actor but you don't have its `ActorRef`
/// - You want to broadcast a message to all actors within a path
///
/// `ActorRef` is almost always the better choice for actor interaction,
/// since messages are directly sent to the actor's mailbox without
/// any preprocessing or cloning.
///
/// `ActorSelection` provides flexibility for the cases where at runtime
/// the `ActorRef`s can't be known. This comes at the cost of traversing
/// part of the actor heirarchy and cloning messages.
///
/// A selection is anchored to an `ActorRef` and the path is relative
/// to that actor's path.
///
/// `selection.try_tell()` is used to message actors in the selection.
/// Since a selection is a collection of `BasicActorRef`s messaging is
/// un-typed. Messages not supported by any actor in the selection will
/// be dropped.
#[derive(Debug)]
pub struct ActorSelection {
    anchor: BasicActorRef,
    // dl: BasicActorRef,
    path_vec: Vec<Selection>,
    path: String,
}

#[derive(Debug)]
enum Selection {
    Parent,
    ChildName(String),
    AllChildren,
}

impl ActorSelection {
    pub fn new(
        anchor: BasicActorRef,
        // dl: &BasicActorRef,
        path: String,
    ) -> Result<ActorSelection, InvalidPath> {
        validate_path(&path)?;

        let path_vec: Vec<Selection> = path
            .split_terminator('/')
            .map({
                |seg| match seg {
                    ".." => Selection::Parent,
                    "*" => Selection::AllChildren,
                    name => Selection::ChildName(name.to_string()),
                }
            })
            .collect();

        Ok(ActorSelection {
            anchor,
            // dl: dl.clone(),
            path_vec,
            path,
        })
    }

    pub async fn try_tell<Msg: Message>(&self, msg: Msg, sender: impl Into<Option<BasicActorRef>>) {
        async fn walk<'a, I, Msg: Message>(
            anchor: &BasicActorRef,
            mut path_vec: Peekable<I>,
            msg: Msg,
            send_out: &Option<BasicActorRef>,
            path: &str,
        ) where
            I: Iterator<Item = &'a Selection> + Send,
        {
            let seg = path_vec.next();

            match seg {
                Some(&Selection::Parent) => {
                    if path_vec.peek().is_none() {
                        let parent = anchor.parent();
                        let _ = parent.try_tell(msg, send_out.clone()).await;
                    } else {
                        Box::pin(walk(&anchor.parent(), path_vec, msg, send_out, path)).await;
                    }
                }
                Some(&Selection::AllChildren) => {
                    for child in anchor.children().iter() {
                        let _ = child.try_tell(msg.clone(), send_out.clone()).await;
                    }
                }
                Some(Selection::ChildName(name)) => {
                    let child = anchor.children().iter().filter(|c| c.name() == name).last();
                    if path_vec.peek().is_none()
                        && let Some(ref child) = child
                    {
                        child.try_tell(msg, send_out.clone()).await.unwrap();
                    } else if path_vec.peek().is_some()
                        && let Some(ref child) = child
                    {
                        Box::pin(walk(child, path_vec, msg, send_out, path)).await;
                    } else {
                        // TODO: send to deadletters?
                    }
                }
                None => {}
            }
        }

        Box::pin(walk(
            &self.anchor,
            // &self.dl,
            self.path_vec.iter().peekable(),
            msg,
            &sender.into(),
            &self.path,
        ))
        .await;
    }

    pub async fn sys_tell(&self, msg: SystemMsg, sender: impl Into<Option<BasicActorRef>>) {
        async fn walk<'a, I>(
            anchor: &BasicActorRef,
            mut path_vec: Peekable<I>,
            msg: SystemMsg,
            send_out: &Option<BasicActorRef>,
            path: &str,
        ) where
            I: Iterator<Item = &'a Selection> + Send,
        {
            let seg = path_vec.next();

            match seg {
                Some(&Selection::Parent) => {
                    if path_vec.peek().is_none() {
                        let parent = anchor.parent();
                        parent.sys_tell(msg).await;
                    } else {
                        Box::pin(walk(&anchor.parent(), path_vec, msg, send_out, path)).await;
                    }
                }
                Some(&Selection::AllChildren) => {
                    for child in anchor.children().iter() {
                        child.sys_tell(msg.clone()).await;
                    }
                }
                Some(Selection::ChildName(name)) => {
                    let child = anchor.children().iter().filter(|c| c.name() == name).last();
                    if path_vec.peek().is_none()
                        && let Some(ref child) = child
                    {
                        child.try_tell(msg, send_out.clone()).await.unwrap();
                    } else if path_vec.peek().is_some()
                        && let Some(ref child) = child
                    {
                        Box::pin(walk(child, path_vec, msg, send_out, path)).await;
                    } else {
                        // TODO: send to deadletters?
                    }
                }
                None => {}
            }
        }

        Box::pin(walk(
            &self.anchor,
            // &self.dl,
            self.path_vec.iter().peekable(),
            msg,
            &sender.into(),
            &self.path,
        ))
        .await;
    }
}

pub trait ActorSelectionFactory {
    fn select(&self, path: &str) -> Result<ActorSelection, InvalidPath>;
}

pub trait RefSelectionFactory {
    fn select_ref(&self, path: &str) -> Option<BasicActorRef>;
}
