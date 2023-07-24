use enet::*;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    net::SocketAddrV4,
    time::{Duration, SystemTime},
};
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
pub trait SocketAgnosticInterface {
    fn send_to_target(&mut self, buf: &[u8], target: String) -> std::io::Result<usize>;
    fn poll_messages(&mut self, buf: &mut [u8]) -> std::io::Result<(usize, String)>;
}

// example SAI for tokio::net::UdpSocket
impl SocketAgnosticInterface for UdpSocket {
    fn send_to_target(&mut self, bytes: &[u8], target: String) -> std::io::Result<usize> {
        let addr = target.parse::<SocketAddr>().unwrap();
        let result = self.try_send_to(&bytes, addr);
        if result.is_err() {
            Err(result.unwrap_err())
        } else {
            Ok(result.unwrap())
        }
    }

    fn poll_messages(&mut self, buf: &mut [u8]) -> std::io::Result<(usize, String)> {
        let result = self.try_recv_from(buf);
        if result.is_err() {
            Err(result.unwrap_err())
        } else {
            let (len, addr) = result.unwrap();
            Ok((len, addr.to_string()))
        }
    }
}

impl SocketAgnosticInterface for enet::Host<()> {
    fn send_to_target(&mut self, buf: &[u8], target: String) -> std::io::Result<usize> {
        let target_addr = Address::from(target.parse::<SocketAddrV4>().unwrap());
        for mut peer in self.peers() {
            if peer.address() == target_addr {
                println!("B");
                let msg = Packet::new(buf, PacketMode::ReliableSequenced).unwrap();
                let result = peer.send_packet(msg, 0);
                if result.is_ok() {
                    return Ok(buf.len());
                } else {
                    return Ok(0);
                }
            }
            continue;
        }
        Ok(buf.len())
    }

    fn poll_messages(&mut self, buf: &mut [u8]) -> std::io::Result<(usize, String)> {
        if let Ok(Some(event)) = self.service(0) {
            match &event {
                Event::Connect(peer) => println!("Peer connected: {:?}", peer.address()),
                Event::Disconnect(peer, _) => println!("Peer disconnected: {:?}", peer.address()),
                Event::Receive {
                    sender,
                    channel_id,
                    packet,
                } => {
                    let data = packet.data();
                    buf[0..data.len()].copy_from_slice(data);
                    println!(
                        "Received packet from: {:?}, len: {}, channel: {}, data: {:?}",
                        sender.address(),
                        data.len(),
                        channel_id,
                        data
                    );
                    return Ok((data.len(), format!("{:?}", sender.address())));
                }
            }
        }
        Ok((0, String::new()))
    }
}
