mod app;
mod client;
mod config;
mod error;
mod local_state;
mod quick_add;
mod ui;

use crate::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load()?;
    let mut app = app::App::new(config)?;
    app.run().await?;
    Ok(())
}
