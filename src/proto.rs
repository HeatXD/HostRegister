use enet::*;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::{
    collections::HashMap,
    net::SocketAddrV4,
    time::{Duration, SystemTime},
};

const PING_INTERVAL: Duration = Duration::from_secs(60);
const PEER_REMOVAL_TIMEMOUT: Duration = Duration::from_secs(120);

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
        self.delete_later = self.last_received_ping.elapsed().unwrap() >= PEER_REMOVAL_TIMEMOUT;
        return self.delete_later;
    }
}

// Socket Agnostic Interface will allow the user to use different types of sockets. important part is that the socket uses the SAI
pub trait SocketAgnosticInterface {
    fn send_to_target(&mut self, data: String, target: String) -> std::io::Result<usize>;
    fn poll_messages(&mut self, buf: &mut [u8]) -> std::io::Result<(usize, String)>;
}

pub struct EnetHost {
    pub enet: enet::Enet,
    pub sock: enet::Host<()>,
    pub peer_activity_map: HashMap<String, SystemTime>,
}

impl EnetHost {
    pub fn check_and_cleanup_clients(&mut self) {
        for peer in self.sock.peers() {
            let addr = EnetAddr {
                addr: peer.address(),
            };
            if let Some(peer_time) = self.peer_activity_map.get(&addr.to_string()) {
                if peer_time.elapsed().unwrap() > PEER_REMOVAL_TIMEMOUT / 4 {
                    peer.disconnect_now(0);
                    println!("Peer disconnected: {}", addr.to_string());
                    self.peer_activity_map.remove(&addr.to_string());
                }
            }
        }
    }
    pub fn init(port: u16, max_concurrent_peers: usize) -> Self {
        let enet = Enet::new().expect("failed to init enet");
        let local_addr = enet::Address::new(Ipv4Addr::LOCALHOST, port);
        let host = enet
            .create_host(
                Some(&local_addr),
                max_concurrent_peers,
                enet::ChannelLimit::Limited(1),
                enet::BandwidthLimit::Unlimited,
                enet::BandwidthLimit::Unlimited,
            )
            .expect("couldn't create host");
        EnetHost {
            enet,
            sock: host,
            peer_activity_map: HashMap::new(),
        }
    }
}

pub struct EnetAddr {
    pub addr: enet::Address,
}

impl Clone for EnetAddr {
    fn clone(&self) -> Self {
        EnetAddr {
            addr: self.addr.clone(),
        }
    }
}

impl ToString for EnetAddr {
    fn to_string(&self) -> String {
        format!("{:?}:{}", self.addr.ip(), self.addr.port())
    }
}

impl SocketAgnosticInterface for EnetHost {
    fn send_to_target(&mut self, data: String, target: String) -> std::io::Result<usize> {
        // have to null terminate the strings for c platforms
        let send_data = data + "\0";
        let buf = send_data.as_bytes();
        let target_addr = Address::from(target.parse::<SocketAddrV4>().unwrap());
        for mut peer in self.sock.peers() {
            if peer.address() == target_addr {
                let msg = Packet::new(buf, PacketMode::ReliableSequenced).unwrap();
                let result = peer.send_packet(msg, 0);
                if result.is_ok() {
                    println!("Sent bytes to: {}, len: {}", target, &buf.len());
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
        if let Ok(Some(event)) = self.sock.service(0) {
            match &event {
                Event::Connect(peer) => {
                    let addr = EnetAddr {
                        addr: peer.address(),
                    };
                    println!("Peer connected: {}", addr.to_string());
                    self.peer_activity_map
                        .insert(addr.to_string(), SystemTime::now());
                }
                Event::Disconnect(peer, _) => {
                    let addr = EnetAddr {
                        addr: peer.address(),
                    };
                    self.peer_activity_map.remove(&addr.to_string());
                    println!("Peer disconnected: {}", addr.to_string())
                }
                Event::Receive {
                    sender,
                    channel_id,
                    packet,
                } => {
                    let addr = EnetAddr {
                        addr: sender.address(),
                    };
                    let data = packet.data();
                    buf[..data.len()].copy_from_slice(data);
                    println!(
                        "Received packet from: {}, len: {}, channel: {}",
                        addr.to_string(),
                        data.len(),
                        channel_id,
                    );
                    // only update timers that have been registered when connecting and havent been removed by being a host.
                    if self.peer_activity_map.contains_key(&addr.to_string()) {
                        self.peer_activity_map
                            .insert(addr.to_string(), SystemTime::now());
                    }
                    return Ok((data.len(), addr.to_string()));
                }
            }
        }
        Ok((0, String::new()))
    }
}
