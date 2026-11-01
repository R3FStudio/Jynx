mod app;
mod updater;
mod installed_runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run().await
}
