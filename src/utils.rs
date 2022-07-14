pub use anyhow::Result;
pub use async_std::{prelude::*, task};
pub use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};

pub use doibak_types::*;

pub type Sender<T> = mpsc::UnboundedSender<T>;
pub type Receiver<T> = mpsc::UnboundedReceiver<T>;

pub fn spawn_and_log_error<F>(fut: F) -> task::JoinHandle<()>
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    task::spawn(async move {
        if let Err(e) = fut.await {
            eprintln!("{}", e)
        }
    })
}

pub trait Distance {
    fn distance(&self, other: &Self) -> usize;
}

impl Distance for (u8, u8) {
    fn distance(&self, other: &Self) -> usize {
        (self.0.abs_diff(other.0) + self.1.abs_diff(other.1)).into()
    }
}

#[derive(Debug)]
pub enum In {
    NewPlayer(String, Sender<Response>, oneshot::Sender<u64>),
    PlayerAction {
        player: u64,
        action: Action,
    },
    Disconnected(u64),
    #[cfg(test)]
    Export(oneshot::Sender<crate::game::GameExport>),
}
