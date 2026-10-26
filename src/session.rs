pub const PROTOCOL_VERSION: i32 = 772;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Handshaking,
    Status,
    Login,
    Configuration,
    Play,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Connected,
    HandshakeSent,
    StatusRequestSent,
    LoginStartSent,
    LoginSuccessReceived,
    LoginAcknowledgedSent,
    FinishConfigurationReceived,
    FinishConfigurationAcknowledged,
    StartConfigurationReceived,
    DisconnectReceived,
    TransportClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTransition {
    pub state: ConnectionState,
    pub event: SessionEvent,
}

#[derive(Debug)]
pub struct SessionMachine {
    state: ConnectionState,
    inbound_packets: u64,
    outbound_packets: u64,
}
impl Default for SessionMachine {
    fn default() -> Self {
        Self { state: ConnectionState::Disconnected, inbound_packets: 0, outbound_packets: 0 }
    }
}
impl SessionMachine {
    pub fn state(&self) -> ConnectionState { self.state }
    pub fn record_inbound(&mut self) { self.inbound_packets = self.inbound_packets.saturating_add(1); }
    pub fn record_outbound(&mut self) { self.outbound_packets = self.outbound_packets.saturating_add(1); }
    pub fn counters(&self) -> (u64, u64) { (self.inbound_packets, self.outbound_packets) }

    pub fn apply(&mut self, event: SessionEvent) -> Result<(), InvalidTransition> {
        use ConnectionState::*;
        use SessionEvent::*;
        let next = match (self.state, event) {
            (Disconnected, Connected) => Connecting,
            (Connecting, HandshakeSent) => Handshaking,
            (Handshaking, StatusRequestSent) => Status,
            (Handshaking, LoginStartSent) => Login,
            (Login, LoginSuccessReceived) => Login,
            (Login, LoginAcknowledgedSent) => Configuration,
            (Configuration, FinishConfigurationReceived) => Configuration,
            (Configuration, FinishConfigurationAcknowledged) => Play,
            (Play, StartConfigurationReceived) => Configuration,
            (_, DisconnectReceived) => Closing,
            (_, TransportClosed) => Disconnected,
            (state, event) => return Err(InvalidTransition { state, event }),
        };
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_state_path() {
        let mut s = SessionMachine::default();
        for event in [
            SessionEvent::Connected, SessionEvent::HandshakeSent,
            SessionEvent::LoginStartSent, SessionEvent::LoginSuccessReceived,
            SessionEvent::LoginAcknowledgedSent,
            SessionEvent::FinishConfigurationReceived,
            SessionEvent::FinishConfigurationAcknowledged,
        ] { s.apply(event).unwrap(); }
        assert_eq!(s.state(), ConnectionState::Play);
    }

    #[test]
    fn rejects_invalid_transition() {
        let mut s = SessionMachine::default();
        assert!(s.apply(SessionEvent::LoginStartSent).is_err());
        assert_eq!(s.state(), ConnectionState::Disconnected);
    }
}
