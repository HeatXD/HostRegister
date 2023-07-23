use nanoid::nanoid;
use proto::SocketAgnosticInterface;
use serde_json::Value;
use std::collections::HashMap;
use std::time::SystemTime;
use std::{io, net::SocketAddr, sync::Arc};
use tokio::{net::UdpSocket, sync::mpsc};

mod proto;

#[tokio::main]
async fn main() -> io::Result<()> {
    // socket setup (could be any socket implementing the SAI in proto.rs)
    let server_addr = String::from("127.0.0.1:8080");
    let sock = UdpSocket::bind(server_addr.parse::<SocketAddr>().unwrap()).await?;
    let r = Arc::new(sock);
    let s = r.clone();
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, String)>(1_000);
    // host id alphabet
    let host_id_length = 8;
    let host_alphabet: [char; 16] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'A', 'B', 'C', 'D', 'E', 'F',
    ];
    // host bookkeeping
    let mut host_register: HashMap<String, proto::Host> = HashMap::new();
    let mut host_map: HashMap<String, String> = HashMap::new();
    // send thread
    tokio::spawn(async move {
        while let Some((bytes, addr)) = rx.recv().await {
            let len = s.send_to_target(&bytes, addr.clone()).await.unwrap();
            println!("{:?} bytes sent to {:?}", len, addr);
        }
    });
    // recv thread / main
    let mut buf = [0; 1024];
    loop {
        // send pings to all hosts to see if theyre still active.
        for (_, host) in &mut host_register {
            if host.should_send_ping() {
                let response = proto::MsgType::PingResponse;
                let data_to_send = serde_json::to_string(&response).unwrap_or_default();
                if !data_to_send.is_empty() {
                    tx.send((data_to_send.into_bytes(), host.addr.clone()))
                        .await
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
        host_register.retain(|_, host| !host.delete_later);
        // poll socket. on err just continue.
        let (len, addr) = r
            .poll_messages(&mut buf)
            .unwrap_or((0, server_addr.clone()));
        if len == 0 {
            continue;
        }
        // get the required slice of the request
        let Ok(data_str) = std::str::from_utf8(&buf[1..len-1]) else {
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
                    tx.send((data_to_send.into_bytes(), addr)).await.unwrap();
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
            }
            // send RegisterResponse
            let response = proto::MsgType::HostRegisterResponse { host_code: id };
            let data_to_send = serde_json::to_string(&response).unwrap_or_default();
            if !data_to_send.is_empty() {
                tx.send((data_to_send.into_bytes(), addr)).await.unwrap();
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
                    tx.send((data_to_send.into_bytes(), addr)).await.unwrap();
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
                    tx.send((data_for_host.into_bytes(), host_info.addr.clone()))
                        .await
                        .unwrap();
                }
            }
            let data_to_send = serde_json::to_string(&response).unwrap_or_default();
            if !data_to_send.is_empty() {
                tx.send((data_to_send.into_bytes(), addr)).await.unwrap();
            }
            continue;
        }
    }
}
