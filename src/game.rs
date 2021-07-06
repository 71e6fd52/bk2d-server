use crate::utils::*;

pub struct Game(pub Receiver<Action>);

impl Game {
    pub async fn main_loop(mut self) {
        while let Some(action) = self.0.next().await {
            println!("{:?}", action)
        }
    }
}
