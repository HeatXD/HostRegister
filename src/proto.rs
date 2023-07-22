use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use std::{net::SocketAddr, time::Duration};

const PING_INTERVAL: Duration = Duration::from_secs(10);
const HOST_REMOVAL_TIMEMOUT: Duration = Duration::from_secs(40);

// Message Types
#[derive(Serialize, Deserialize)]
#[serde(tag = "msg_type")]
pub enum MsgType {
    PingRequest,
    PingResponse,
    HostRegisterRequest,
    HostRegisterResponse { host_code: String },
    HostLookupRequest { host_code: String },
    HostLookupResponse { success: bool, host_info: String },
}

pub struct Host {
    pub id: String,
    pub addr: SocketAddr,
    pub last_sent_ping: SystemTime,
    pub last_received_ping: SystemTime,
    pub delete_later: bool,
}

impl Host {
    pub fn should_send_ping(&mut self) -> bool {
        let should_ping = self.last_sent_ping.elapsed().unwrap() > PING_INTERVAL;
        if should_ping {
            self.last_sent_ping = SystemTime::now();
        }
        return should_ping;
    }

    pub fn should_be_removed(&mut self) -> bool {
        self.delete_later = self.last_received_ping.elapsed().unwrap() >= HOST_REMOVAL_TIMEMOUT;
        return self.delete_later;
    }
}
