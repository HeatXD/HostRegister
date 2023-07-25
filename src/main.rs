use nanoid::nanoid;
use proto::EnetAddr;
use proto::EnetHost;
use proto::SocketAgnosticInterface;
use serde_json::Value;
use std::collections::HashMap;
use std::time::SystemTime;

mod proto;

fn main() {
    // socket setup (could be any socket implementing the SAI in proto.rs)
    let mut enet_host = EnetHost::init(4422, 500);
    // host id alphabet
    let host_id_length = 8;
    let host_alphabet: [char; 16] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'A', 'B', 'C', 'D', 'E', 'F',
    ];
    // host bookkeeping
    let mut host_register: HashMap<String, proto::Host> = HashMap::new();
    let mut host_map: HashMap<String, String> = HashMap::new();
    
    let mut buf = [0; 1024];
    loop {
        // cleanup clients
        enet_host.check_and_cleanup_clients();
        // send pings to all hosts to see if theyre still active.
        for (_, host) in &mut host_register {
            if host.should_send_ping() {
                let response = proto::MsgType::PingResponse;
                let data_to_send = serde_json::to_string(&response).unwrap_or_default();
                if !data_to_send.is_empty() {
                    enet_host.send_to_target(data_to_send.as_bytes(), host.addr.clone())
                        .unwrap();
                }
            }
            // check if cleanup is needed.
            if host.should_be_removed() {
                host_map.remove(&host.addr);
                println!("Removed {:?} as an available host. Timed out.", &host.addr);
            }
        }
        // clean up the hosts that have timed out.
        host_register.retain(|_, host| {
            if host.delete_later {
                // disconnect from enet
                for peer in enet_host.sock.peers() {
                    let addr = EnetAddr {
                        addr: peer.address(),
                    };
                    if addr.to_string() == host.addr {
                        peer.disconnect_now(0);
                        break;
                    }
                }
            }
            !host.delete_later
        });
        // poll socket. on err just continue.
        let (len, addr) = enet_host.poll_messages(&mut buf).unwrap_or((0, String::new()));
        if len == 0 {
            continue;
        }
        // get the required slice of the request
        let Ok(data_str) = std::str::from_utf8(&buf[..len]) else {
            continue;
        };
        // parse the string of data into serde_json::Value.
        let request: Value = serde_json::from_str(&data_str).unwrap_or_default();
        if request == Value::Null {
            continue;
        }
        // handle request concurrently if available
        if request["msg_type"] == Value::Null {
            continue;
        }
        // Ping
        if request["msg_type"] == "PingRequest" {
            // check if ping comes from a host
            let host_id = host_map.get(&addr);
            if host_id.is_none() {
                continue;
            }
            if let Some(host_info) = host_register.get_mut(host_id.unwrap()) {
                host_info.last_received_ping = SystemTime::now();
                let response = proto::MsgType::PingResponse;
                let data_to_send = serde_json::to_string(&response).unwrap_or_default();
                if !data_to_send.is_empty() {
                    enet_host.send_to_target(data_to_send.as_bytes(), addr.clone())
                        .unwrap();
                }
            }
            continue;
        }
        // HostRegisterRequest
        if request["msg_type"] == "HostRegisterRequest" {
            let mut id = nanoid!(host_id_length, &host_alphabet);
            // check if the host_map already has an id for this socket
            if let Some(host_id) = host_map.get(&addr) {
                id = host_id.clone();
                if let Some(host) = host_register.get_mut(&id) {
                    host.last_received_ping = SystemTime::now();
                }
            } else {
                while host_register.get(&id).is_some() {
                    id = nanoid!(host_id_length, &host_alphabet);
                }
                // add new host to the register with the generated id
                let now = SystemTime::now();
                let new_host = proto::Host {
                    id: id.clone(),
                    addr: addr.clone(),
                    last_sent_ping: now,
                    last_received_ping: now,
                    delete_later: false,
                };
                // add to registers.
                host_register.insert(id.clone(), new_host);
                host_map.insert(addr.clone(), id.clone());
                println!("Added {:?} to the host register.", &addr);
                // remove it from the activity map since its a known host. the map should only be for clients
                enet_host.peer_activity_map.remove(&addr);
            }
            // send RegisterResponse
            let response = proto::MsgType::HostRegisterResponse { host_code: id };
            let data_to_send = serde_json::to_string(&response).unwrap_or_default();
            if !data_to_send.is_empty() {
                enet_host.send_to_target(data_to_send.as_bytes(), addr.clone())
                    .unwrap();
            }
            continue;
        }
        // HostLookupRequest
        if request["msg_type"] == "HostLookupRequest" {
            let id_to_find = request["host_code"].as_str().unwrap_or_default();
            let mut response = proto::MsgType::HostLookupResponse {
                success: false,
                host_info: String::new(),
            };
            if id_to_find.is_empty() {
                // no hostcode send failed response.
                let data_to_send = serde_json::to_string(&response).unwrap_or_default();
                if !data_to_send.is_empty() {
                    enet_host.send_to_target(data_to_send.as_bytes(), addr.clone())
                        .unwrap();
                }
                continue;
            }
            // if host_register has the wanted host. send a response with the host info.
            if let Some(host_info) = host_register.get(id_to_find) {
                response = proto::MsgType::HostLookupResponse {
                    success: true,
                    host_info: host_info.addr.to_string(),
                };
                // and send ClientLookupResponse to the host
                let host_response = proto::MsgType::ClientLookupResponse {
                    client_info: addr.to_string(),
                };
                let data_for_host = serde_json::to_string(&host_response).unwrap_or_default();
                if !data_for_host.is_empty() {
                    enet_host.send_to_target(data_for_host.as_bytes(), host_info.addr.clone())
                        .unwrap();
                }
            }
            let data_to_send = serde_json::to_string(&response).unwrap_or_default();
            if !data_to_send.is_empty() {
                enet_host.send_to_target(data_to_send.as_bytes(), addr.clone())
                    .unwrap();
            }
            continue;
        }
    }
}
