pub use anyhow::Result;
pub use async_std::{prelude::*, task};
pub use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use serde::{Deserialize, Serialize};
use std::fmt;

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

#[derive(Debug)]
pub enum In {
    NewPlayer(String, Sender<Response>, oneshot::Sender<u64>),
    PlayerAction {
        player: u64,
        action: Action,
    },
    #[cfg(test)]
    Export(oneshot::Sender<crate::game::GameExport>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    CreateRoom { name: String },
    JoinRoom { id: u64 },
    Ready(u8, u8),
    Game(GameAction),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameAction {
    Move(u8, u8),
    Attack(u8, u8),
    Run(u8, u8),
    End,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Response {
    Error(Error),
    RoomCreated(u64),
    RoomJoined,
    GameStarted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "error_id", content = "error_detail")]
#[serde(rename_all = "snake_case")]
pub enum Error {
    RoomNotFound,
    NotJoinRoom,
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.serialize(f)
    }
}
