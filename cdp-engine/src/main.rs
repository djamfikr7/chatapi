use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber::EnvFilter;

use chatapi_cdp_engine::{CdpConnection, ChatEngine};
use chatapi_cdp_engine::discovery;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,chatapi_cdp_engine=debug")),
        )
        .init();

    info!("ChatAPI CDP Engine starting");

    // Discover Chrome's DevTools endpoint.
    let port_file = discovery::find_chrome_socket()?;
    let (port, _ws_path) = discovery::parse_devtools_active_port(&port_file)?;
    info!("Chrome debug port: {}", port);

    // Get the full WebSocket URL.
    let ws_url = discovery::get_debug_ws_url(port).await?;
    info!("Connecting to: {}", ws_url);

    // Connect to Chrome via CDP.
    let cdp = CdpConnection::connect(&ws_url).await?;

    // Create and initialize the chat engine.
    let mut engine = ChatEngine::new(cdp);
    engine.initialize().await?;

    info!("Chat engine ready. Sending test prompt...");

    // Example: send a prompt and print the response.
    match engine.chat("Hello, what model are you?").await {
        Ok(response) => {
            info!("Response:\n{}", response);
            println!("{}", response);
        }
        Err(e) => {
            error!("Chat failed: {}", e);
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
