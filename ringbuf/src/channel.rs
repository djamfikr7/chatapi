use tokio::sync::mpsc;

/// Commands sent from the gateway to the CDP engine.
#[derive(Debug, Clone)]
pub enum CdpCommand {
    SendPrompt {
        session_id: String,
        prompt: String,
    },
    NewSession,
    CloseSession,
}

/// Events emitted by the CDP engine back to the gateway.
#[derive(Debug, Clone)]
pub enum CdpEvent {
    TokenReceived {
        session_id: String,
        token: String,
    },
    StreamComplete {
        session_id: String,
    },
    Error {
        session_id: String,
        message: String,
    },
}

/// Creates a paired command/event channel.
///
/// `CommandSender` is held by the gateway to dispatch commands.
/// `EventReceiver` is held by the gateway to consume CDP events.
pub struct CommandChannel;

impl CommandChannel {
    pub fn new(buffer: usize) -> (CommandSender, EventReceiver) {
        let (cmd_tx, cmd_rx) = mpsc::channel(buffer);
        let (evt_tx, evt_rx) = mpsc::channel(buffer);
        (
            CommandSender {
                tx: cmd_tx,
                evt_tx,
            },
            EventReceiver { rx: evt_rx, cmd_rx },
        )
    }
}

/// Gateway-side handle: send commands and receive events.
pub struct CommandSender {
    tx: mpsc::Sender<CdpCommand>,
    evt_tx: mpsc::Sender<CdpEvent>,
}

impl CommandSender {
    /// Send a command to the CDP engine.
    pub async fn send_command(&self, cmd: CdpCommand) -> Result<(), ChannelError> {
        self.tx.send(cmd).await.map_err(|_| ChannelError::Closed)
    }

    /// Push an event from the CDP engine (used by the engine side).
    pub async fn send_event(&self, evt: CdpEvent) -> Result<(), ChannelError> {
        self.evt_tx.send(evt).await.map_err(|_| ChannelError::Closed)
    }
}

/// Gateway-side handle: receive events and send commands back to the engine.
pub struct EventReceiver {
    rx: mpsc::Receiver<CdpEvent>,
    cmd_rx: mpsc::Receiver<CdpCommand>,
}

impl EventReceiver {
    /// Wait for the next CDP event.
    pub async fn recv_event(&mut self) -> Option<CdpEvent> {
        self.rx.recv().await
    }

    /// Wait for the next command (used by the engine side).
    pub async fn recv_command(&mut self) -> Option<CdpCommand> {
        self.cmd_rx.recv().await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("channel closed")]
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn command_roundtrip() {
        let (tx, mut rx) = CommandChannel::new(8);
        tx.send_command(CdpCommand::NewSession).await.unwrap();
        let cmd = rx.recv_command().await.unwrap();
        assert!(matches!(cmd, CdpCommand::NewSession));
    }

    #[tokio::test]
    async fn event_roundtrip() {
        let (tx, mut rx) = CommandChannel::new(8);
        tx.send_event(CdpEvent::TokenReceived {
            session_id: "s1".into(),
            token: "hi".into(),
        })
        .await
        .unwrap();
        let evt = rx.recv_event().await.unwrap();
        match evt {
            CdpEvent::TokenReceived { session_id, token } => {
                assert_eq!(session_id, "s1");
                assert_eq!(token, "hi");
            }
            _ => panic!("unexpected event variant"),
        }
    }
}
