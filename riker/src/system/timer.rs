use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use chrono::{DateTime, Utc};
use config::Config;
use uuid::Uuid;

use crate::{
    actor::{ActorRef, BasicActorRef},
    AnyMessage, Message,
};

pub type TimerRef = mpsc::Sender<Job>;

pub type ScheduleId = Uuid;

#[async_trait::async_trait]
pub trait Timer {
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
        M: Message;

    async fn schedule_once<T, M>(
        &self,
        delay: Duration,
        receiver: ActorRef<M>,
        send_out: Option<BasicActorRef>,
        msg: T,
    ) -> ScheduleId
    where
        T: Message + Into<M>,
        M: Message;

    async fn schedule_at_time<T, M>(
        &self,
        time: DateTime<Utc>,
        receiver: ActorRef<M>,
        send_out: Option<BasicActorRef>,
        msg: T,
    ) -> ScheduleId
    where
        T: Message + Into<M>,
        M: Message;

    async fn cancel_schedule(&self, id: Uuid);
}

pub enum Job {
    Once(OnceJob),
    Repeat(RepeatJob),
    Cancel(Uuid),
}

pub struct OnceJob {
    pub id: Uuid,
    pub send_at: Instant,
    pub receiver: BasicActorRef,
    pub send_out: Option<BasicActorRef>,
    pub msg: AnyMessage,
}

impl OnceJob {
    pub async fn send(mut self) {
        let _ = self
            .receiver
            .try_tell_any(&mut self.msg, self.send_out)
            .await;
    }
}

pub struct RepeatJob {
    pub id: Uuid,
    pub send_at: Instant,
    pub interval: Duration,
    pub receiver: BasicActorRef,
    pub send_out: Option<BasicActorRef>,
    pub msg: AnyMessage,
}

impl RepeatJob {
    pub async fn send(&mut self) {
        let _ = self
            .receiver
            .try_tell_any(&mut self.msg, self.send_out.clone())
            .await;
    }
}

// Default timer implementation

pub struct BasicTimer {
    once_jobs: Vec<OnceJob>,
    repeat_jobs: Vec<RepeatJob>,
}

impl BasicTimer {
    pub fn start(cfg: &Config) -> TimerRef {
        let cfg = BasicTimerConfig::from(cfg);

        let mut process = BasicTimer {
            once_jobs: Vec::new(),
            repeat_jobs: Vec::new(),
        };

        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            loop {
                process.execute_once_jobs().await;
                process.execute_repeat_jobs().await;

                if let Ok(job) = rx.try_recv() {
                    match job {
                        Job::Cancel(id) => process.cancel(&id),
                        Job::Once(job) => process.schedule_once(job).await,
                        Job::Repeat(job) => process.schedule_repeat(job).await,
                    }
                }
                tokio::time::sleep(Duration::from_millis(cfg.frequency_millis)).await;
            }
        });

        tx
    }

    pub async fn execute_once_jobs(&mut self) {
        let (send, keep): (Vec<OnceJob>, Vec<OnceJob>) = self
            .once_jobs
            .drain(..)
            .partition(|j| Instant::now() >= j.send_at);

        // send those messages where the 'send_at' time has been reached or elapsed
        for job in send {
            job.send().await;
        }

        // for those messages that are not to be sent yet, just put them back on the vec
        for job in keep {
            self.once_jobs.push(job);
        }
    }

    pub async fn execute_repeat_jobs(&mut self) {
        for job in self.repeat_jobs.iter_mut() {
            if Instant::now() >= job.send_at {
                job.send_at = Instant::now() + job.interval;
                job.send().await;
            }
        }
    }

    pub fn cancel(&mut self, id: &Uuid) {
        // slightly sub optimal way of canceling because we don't know the job type
        // so need to do the remove on both vecs

        if let Some(pos) = self.once_jobs.iter().position(|job| &job.id == id) {
            self.once_jobs.remove(pos);
        }

        if let Some(pos) = self.repeat_jobs.iter().position(|job| &job.id == id) {
            self.repeat_jobs.remove(pos);
        }
    }

    pub async fn schedule_once(&mut self, job: OnceJob) {
        if Instant::now() >= job.send_at {
            job.send().await;
        } else {
            self.once_jobs.push(job);
        }
    }

    pub async fn schedule_repeat(&mut self, mut job: RepeatJob) {
        if Instant::now() >= job.send_at {
            job.send().await;
        }
        self.repeat_jobs.push(job);
    }
}

struct BasicTimerConfig {
    frequency_millis: u64,
}

impl<'a> From<&'a Config> for BasicTimerConfig {
    fn from(config: &Config) -> Self {
        BasicTimerConfig {
            frequency_millis: config.get_int("scheduler.frequency_millis").unwrap() as u64,
        }
    }
}
