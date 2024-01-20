use config::Config;
use futures::{
    task::{Context as PollContext, Poll},
    Future,
};
use std::{error::Error, pin::Pin, sync::Arc};
use tokio::sync::oneshot::Receiver;

pub type ExecutorHandle = Arc<dyn TaskExecutor>;

pub type TaskError = Box<dyn Error + Send + 'static>;

pub trait Task: Future<Output = ()> + Send {}
impl<T: Future<Output = ()> + Send> Task for T {}

pub trait TaskExecutor {
    fn spawn(&self, future: Pin<Box<dyn Task>>) -> Result<Box<dyn TaskExec<()>>, TaskError>;
}

pub trait TaskExec<T: Send>: Future<Output = Result<T, TaskError>> + Unpin + Send + Sync {
    fn abort(self: Box<Self>);

    fn forget(self: Box<Self>);
}

pub struct TaskHandle<T: Send> {
    handle: Box<dyn TaskExec<()>>,
    recv: Receiver<T>,
}

impl<T: Send> TaskHandle<T> {
    pub fn new(handle: Box<dyn TaskExec<()>>, recv: Receiver<T>) -> Self {
        Self { handle, recv }
    }
}

impl<T: Send> Future for TaskHandle<T> {
    type Output = Result<T, TaskError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut PollContext<'_>) -> Poll<Self::Output> {
        if Pin::new(&mut *self.handle).poll(cx).is_ready() {
            if let Poll::Ready(val) = <Receiver<T> as Future>::poll(Pin::new(&mut self.recv), cx) {
                self.recv.close();
                return Poll::Ready(
                    val.map_err(|e| Box::new(e) as Box<dyn Error + Send + 'static>),
                );
            }
        }
        Poll::Pending
    }
}

impl<T: Send> TaskHandle<T> {
    pub fn abort(self) {
        self.handle.abort()
    }

    pub fn forget(self) {
        self.handle.forget()
    }
}
impl<T: Send> TaskExec<T> for TaskHandle<T> {
    fn abort(self: Box<Self>) {
        self.handle.abort()
    }

    fn forget(self: Box<Self>) {
        self.handle.forget()
    }
}

pub fn get_executor_handle(_: &Config) -> ExecutorHandle {
    Arc::new(TokioExecutor(tokio::runtime::Handle::current()))
}

pub struct TokioExecutor(pub tokio::runtime::Handle);

impl TaskExecutor for TokioExecutor {
    fn spawn(&self, future: Pin<Box<dyn Task>>) -> Result<Box<dyn TaskExec<()>>, TaskError> {
        Ok(Box::new(TokioJoinHandle(self.0.spawn(future))))
    }
}

struct TokioJoinHandle(tokio::task::JoinHandle<()>);

impl Future for TokioJoinHandle {
    type Output = Result<(), TaskError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut PollContext<'_>) -> Poll<Self::Output> {
        Future::poll(Pin::new(&mut self.0), cx)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + 'static>)
    }
}

impl TaskExec<()> for TokioJoinHandle {
    fn abort(self: Box<Self>) {
        self.0.abort();
    }

    fn forget(self: Box<Self>) {
        drop(self);
    }
}
