use std::future::Future;

use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

pub trait Task: Send + 'static {
    type I: Send;
    type O: Send;

    fn run(&mut self, value: Self::I) -> impl Future<Output = Self::O> + Send;
}

pub struct Runner<T: Task> {
    #[allow(dead_code)]
    handle: JoinHandle<()>,
    tx: mpsc::Sender<(T::I, oneshot::Sender<T::O>)>,
    cx: mpsc::Sender<()>,
}

impl<T: Task> Runner<T> {
    pub fn new(mut task: T) -> Self {
        let (tx, mut rx) = mpsc::channel::<(T::I, oneshot::Sender<T::O>)>(64);
        let (cx, mut cx_rx) = mpsc::channel(1);
        let handle = tokio::spawn(async move {
            while let Some((input, ret)) = rx.recv().await {
                select! {
                    o = task.run(input) => {
                        let _ = ret.send(o);
                    },
                    _ = cx_rx.recv() => {},
                }
            }
        });

        Self { handle, tx, cx }
    }

    pub async fn run(&self, input: T::I) -> Option<T::O> {
        let (tx, rx) = oneshot::channel();
        self.tx.send((input, tx)).await.unwrap();
        rx.await.ok()
    }

    pub async fn cancel(&self) {
        self.cx.send(()).await.unwrap();
    }
}
