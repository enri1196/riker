#![feature(let_chains)]

pub mod actor;
pub mod ask;
pub mod kernel;
pub mod system;
pub mod validate;

use std::any::Any;
use std::env;
use std::fmt;
use std::fmt::Debug;

use config::{Config, File};

use crate::actor::BasicActorRef;

pub fn load_config() -> Config {
    let riker_conf_path = env::var("RIKER_CONF").unwrap_or_else(|_| "config/riker.toml".into());
    let app_conf_path = env::var("APP_CONF").unwrap_or_else(|_| "config/app".into());

    Config::builder()
        .set_default("mailbox.msg_process_limit", 1000)
        .unwrap()
        .set_default("dispatcher.pool_size", (num_cpus::get() * 2) as i64)
        .unwrap()
        .set_default("dispatcher.stack_size", 0)
        .unwrap()
        .set_default("scheduler.frequency_millis", 50)
        .unwrap()
        // load the system config
        // riker.toml contains settings for anything related to the actor framework and its modules
        .add_source(File::with_name(&riker_conf_path).required(false))
        // load the user application config
        // app.toml or app.yaml contains settings specific to the user application
        .add_source(File::with_name(&app_conf_path).required(false))
        .build()
        .unwrap()
}

/// Wraps message and sender
#[derive(Debug, Clone)]
pub struct Envelope<T: Message> {
    pub send_out: Option<BasicActorRef>,
    pub msg: T,
}

unsafe impl<T: Message> Send for Envelope<T> {}

pub trait Message: Debug + Clone + Send + 'static {}
impl<T: Debug + Clone + Send + 'static> Message for T {}

pub struct AnyMessage {
    pub one_time: bool,
    pub msg: Option<Box<dyn Any + Send>>,
}

pub struct DowncastAnyMessageError;

impl AnyMessage {
    pub fn new<T>(msg: T, one_time: bool) -> Self
    where
        T: Any + Message,
    {
        Self {
            one_time,
            msg: Some(Box::new(msg)),
        }
    }

    pub fn take<T>(&mut self) -> Result<T, DowncastAnyMessageError>
    where
        T: Any + Message,
    {
        if self.one_time {
            match self.msg.take() {
                Some(m) => {
                    if m.is::<T>() {
                        Ok(*m.downcast::<T>().unwrap())
                    } else {
                        Err(DowncastAnyMessageError)
                    }
                }
                None => Err(DowncastAnyMessageError),
            }
        } else {
            match self.msg.as_ref() {
                Some(m) if m.is::<T>() => Ok(m.downcast_ref::<T>().cloned().unwrap()),
                Some(_) => Err(DowncastAnyMessageError),
                None => Err(DowncastAnyMessageError),
            }
        }
    }
}

impl Clone for AnyMessage {
    fn clone(&self) -> Self {
        panic!("Can't clone a message of type `AnyMessage`");
    }
}

impl Debug for AnyMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("AnyMessage")
    }
}

pub mod actors {
    pub use crate::actor::*;
    pub use crate::ask::*;
    pub use crate::system::{
        ActorSystem, Run, ScheduleId, SystemBuilder, SystemEvent, SystemMsg, Timer,
    };
    pub use crate::{AnyMessage, Message};
    pub use crate::kernel::mailbox::AnyEnqueueError;
}
