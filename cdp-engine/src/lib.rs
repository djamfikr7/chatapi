pub mod cdp;
pub mod discovery;
pub mod engine;

pub use cdp::CdpConnection;
pub use discovery::{find_chrome_socket, get_debug_ws_url};
pub use engine::ChatEngine;
