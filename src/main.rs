use anyhow::bail;
use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Arc,
};

mod game;
mod utils;
use utils::*;

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    let (game_sender, game_receiver) = mpsc::unbounded();
    let game = game::Game::new(game_receiver);
    let _game_handle = task::spawn(game.main_loop());

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("Accepting from: {}", stream.peer_addr()?);
        spawn_and_log_error(connection_loop(game_sender.clone(), stream));
    }
    Ok(())
}

async fn connection_loop(mut game: Sender<In>, stream: TcpStream) -> Result<()> {
    use In::*;

    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();

    let (mut response_sender, response_receiver) = mpsc::unbounded();
    spawn_and_log_error(connection_writer_loop(response_receiver, stream.clone()));

    let player = match lines.next().await {
        None => bail!("peer disconnected immediately"),
        Some(line) => line?,
    };
    let (id_sender, id_receiver) = oneshot::channel();
    game.send(NewPlayer(player, response_sender.clone(), id_sender))
        .await?;
    let player = id_receiver.await?;

    while let Some(line) = lines.next().await {
        let line = line?;
        match serde_lexpr::from_str(&line) {
            Ok(data) => {
                game.send(PlayerAction {
                    player,
                    action: data,
                })
                .await?
            }
            Err(err) => {
                response_sender
                    .send(Response::Error(err.to_string()))
                    .await?
            }
        }
    }
    Ok(())
}

async fn connection_writer_loop(
    mut messages: Receiver<Response>,
    stream: Arc<TcpStream>,
) -> Result<()> {
    let mut stream = &*stream;
    while let Some(msg) = messages.next().await {
        stream
            .write_all((serde_lexpr::to_string(&msg)? + "\n").as_bytes())
            .await?;
    }
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let fut = accept_loop("127.0.0.1:27933");
    task::block_on(fut)
}
