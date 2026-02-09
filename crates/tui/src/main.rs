use sparagne_tui::{Result, app::App, config};

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load()?;
    let mut app = App::new(config)?;
    app.run().await?;
    Ok(())
}
