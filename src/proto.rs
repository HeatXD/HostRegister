use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use tokio::net::UdpSocket;

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
    ClientLookupResponse { client_info: String },
}

// Host Information
pub struct Host {
    pub id: String,
    pub addr: String,
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

// Socket Agnostic Interface will allow the user to use different types of sockets. important part is that the socket uses the SAI
#[async_trait]
pub trait SocketAgnosticInterface {
    async fn send_to_target(&self, buf: &[u8], target: String) -> std::io::Result<usize>;
    fn poll_messages(&self, buf: &mut [u8]) -> std::io::Result<(usize, String)>;
}

// example SAI for tokio::net::UdpSocket
#[async_trait]
impl SocketAgnosticInterface for UdpSocket {
    async fn send_to_target(&self, bytes: &[u8], target: String) -> std::io::Result<usize> {
        self.send_to(&bytes, &target).await
    }

    fn poll_messages(&self, buf: &mut [u8]) -> std::io::Result<(usize, String)> {
        let result = self.try_recv_from(buf);
        if result.is_err() {
            Err(result.unwrap_err())
        } else {
            let (len, addr) = result.unwrap();
            Ok((len, addr.to_string()))
        }
    }
}
