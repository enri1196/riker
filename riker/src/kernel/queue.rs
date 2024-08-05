use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

use crate::{Envelope, Message};

pub fn queue<Msg: Message>() -> (QueueWriter<Msg>, QueueReader<Msg>) {
    let (tx, rx) = channel::<Envelope<Msg>>(100);

    let qw = QueueWriter { tx };

    let qr = QueueReaderInner {
        rx,
        next_item: None,
    };

    let qr = QueueReader {
        inner: Mutex::new(qr),
    };

    (qw, qr)
}

#[derive(Clone)]
pub struct QueueWriter<Msg: Message> {
    tx: Sender<Envelope<Msg>>,
}

impl<Msg: Message> QueueWriter<Msg> {
    pub async fn try_enqueue(&self, msg: Envelope<Msg>) -> EnqueueResult<Msg> {
        self.tx
            .send(msg)
            .await
            .map_err(|e| EnqueueError { msg: e.0 })
    }
}

pub struct QueueReader<Msg: Message> {
    inner: Mutex<QueueReaderInner<Msg>>,
}

struct QueueReaderInner<Msg: Message> {
    rx: Receiver<Envelope<Msg>>,
    next_item: Option<Envelope<Msg>>,
}

impl<Msg: Message> QueueReader<Msg> {
    pub async fn dequeue(&self) -> Envelope<Msg> {
        let mut inner = self.inner.lock().await;
        if let Some(item) = inner.next_item.take() {
            item
        } else {
            inner.rx.recv().await.unwrap()
        }
    }

    pub async fn try_dequeue(&self) -> DequeueResult<Envelope<Msg>> {
        let mut inner = self.inner.lock().await;
        if let Some(item) = inner.next_item.take() {
            Ok(item)
        } else {
            inner.rx.try_recv().map_err(|_| QueueEmpty)
        }
    }

    pub async fn has_msgs(&self) -> bool {
        let mut inner = self.inner.lock().await;
        inner.next_item.is_some() || {
            match inner.rx.try_recv() {
                Ok(item) => {
                    inner.next_item = Some(item);
                    true
                }
                Err(_) => false,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnqueueError<T> {
    pub msg: T,
}

pub type EnqueueResult<Msg> = Result<(), EnqueueError<Envelope<Msg>>>;

pub struct QueueEmpty;
pub type DequeueResult<Msg> = Result<Msg, QueueEmpty>;
