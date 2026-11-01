mod app;
mod installed_runtime;
mod terminal_ui;
mod updater;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run().await
}
