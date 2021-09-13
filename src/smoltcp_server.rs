pub mod utils;

use argparse::{ArgumentParser, Store, StoreTrue};
use smoltcp::socket::{UdpPacketMetadata, UdpSocket, UdpSocketBuffer};
use socket2::{Domain, Socket, Type};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use smoltcp::wire::{IpAddress, IpCidr};

struct KcpOutput {
    socket: Arc<std::net::UdpSocket>,
    src: std::net::SocketAddr,
}

impl Read for KcpOutput {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        Ok(self.socket.recv(data).unwrap())
    }
}

fn main() {
    let mut address = "0.0.0.0".to_string();
    const BUCKET_SIZE: usize = 64 * 1024;
    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("tcp server.");
        ap.refer(&mut address)
            .add_option(&["--address"], Store, "Listening address");
        ap.parse_args_or_exit();
    }

    let mut workers = Vec::new();
    let listen_address = address.clone();

    println!("listen_address={:?}", listen_address);
    let listen_address = format!("{}:0", listen_address);
    // // let listen_to_address = format!("{}:0", *address);

    let udp_rx_buffer = UdpSocketBuffer::new(
        vec![UdpPacketMetadata::EMPTY; 4],
        vec![0; 32 * (1024 as usize).pow(2)],
    );
    let udp_tx_buffer = UdpSocketBuffer::new(vec![UdpPacketMetadata::EMPTY], vec![0; 1024]);
    let mut udp_socket = UdpSocket::new(udp_rx_buffer, udp_tx_buffer);
    let mut sockaddr: SocketAddr = listen_address.parse().unwrap();
    {
        let sock = std::net::UdpSocket::bind(listen_address).unwrap();
        sockaddr = sock.local_addr().unwrap();
    }

    udp_socket.bind(sockaddr).unwrap();
    println!("Listening on {:?}", udp_socket.endpoint());

    let cidrs: Vec<IpCidr> = utils::find_interfaces().iter().map(|x| x.ip_cidr).collect();

    let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
    let mut log_count = 0;
    workers.push(std::thread::spawn(move || loop {
        let (recv_bytes, src_addr) = match udp_socket.recv_slice(&mut bucket[..]) {
            Ok(ok) => ok,
            Err(err) => match err {
                smoltcp::Error::Exhausted => continue,
                _ => {
                    panic!("{:?}", err);
                }
            }
        };

        log_count += 1;
        if log_count % 100000 == 0 {
            println!("src_addr={:?}, recv_bytes={}", src_addr, recv_bytes);
        }
    }));

    for worker in workers {
        worker.join().unwrap();
    }
}
