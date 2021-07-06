use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Arc,
};
use futures::sink::SinkExt;

mod game;
mod utils;
use utils::*;

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    let (game_sender, game_receiver) = mpsc::unbounded();
    let mut game = game::Game(game_receiver);
    let _game_handle = task::spawn(game.main_loop());

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("Accepting from: {}", stream.peer_addr()?);
        spawn_and_log_error(connection_loop(game_sender.clone(), stream));
    }
    Ok(())
}

async fn connection_loop(mut game: Sender<Action>, stream: TcpStream) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    // let name = match lines.next().await {
    //     None => Err("peer disconnected immediately")?,
    //     Some(line) => line?,
    // };
    // broker
    //     .send(Event::NewPeer {
    //         name: name.clone(),
    //         stream: Arc::clone(&stream),
    //     })
    //     .await // 3
    //     .unwrap();

    while let Some(line) = lines.next().await {
        let line = line?;
        game.send(serde_lexpr::from_str(&line)?).await?;
    }
    Ok(())
}

async fn connection_writer_loop(
    mut messages: Receiver<String>,
    stream: Arc<TcpStream>,
) -> Result<()> {
    let mut stream = &*stream;
    while let Some(msg) = messages.next().await {
        stream.write_all(msg.as_bytes()).await?;
    }
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let fut = accept_loop("127.0.0.1:27933");
    task::block_on(fut)
}
