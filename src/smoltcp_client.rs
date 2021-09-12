pub mod utils;

use argparse::{ArgumentParser, Store, StoreTrue};
use pbr::{MultiBar, ProgressBar};
use smoltcp::socket::{SocketSet, UdpPacketMetadata, UdpSocket, UdpSocketBuffer};
// use smoltcp::iface::{InterfaceBuilder, NeighborCache, Routes};
use smoltcp::wire::IpEndpoint;
use socket2::{Domain, Socket, Type};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

struct KcpOutput {
    socket: Arc<Mutex<std::net::UdpSocket>>,
    src: std::net::SocketAddr,
}

impl Write for KcpOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        Ok(self
            .socket
            .lock()
            .unwrap()
            .send_to(data, &self.src)
            .unwrap())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct UdpOutput {
    socket: std::net::UdpSocket,
    src: std::net::SocketAddr,
}

impl Write for UdpOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        Ok(self.socket.send(data).unwrap())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn main() {
    let mut address = "127.0.0.1:63590".to_string();
    let mut bucket_size: usize = 32768;
    let mut repeat = 100000;
    let mut nstreams = 1;
    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("tcp server.");
        ap.refer(&mut address)
            .add_option(&["--server_address"], Store, "Server address");
        ap.refer(&mut bucket_size)
            .add_option(&["--bucket_size"], Store, "Bucket size");
        ap.refer(&mut repeat)
            .add_option(&["--repeat"], Store, "repeat count");
        ap.refer(&mut nstreams)
            .add_option(&["--nstreams"], Store, "num of stream");
        ap.parse_args_or_exit();
    }

    let server_sockaddr: SocketAddr = address.parse().expect(&format!("address={}", address));
    let server_endpoint: IpEndpoint = server_sockaddr.into();

    let ip_addrs = utils::find_interfaces();
    println!("{:?}", ip_addrs);

    println!("server_endpoint={:?}", server_endpoint);

    // let multi_bar = MultiBar::new();
    let mut workers = Vec::new();
    for _ in 0..nstreams {
        let bucket: Vec<u8> = vec![0; bucket_size];
        // let mut progress = multi_bar.create_bar(repeat);
        let udp_rx_buffer = UdpSocketBuffer::new(
            vec![UdpPacketMetadata::EMPTY; 4],
            vec![0; 32 * (1024 as usize).pow(2)],
        );
        let udp_tx_buffer = UdpSocketBuffer::new(
            vec![UdpPacketMetadata::EMPTY],
            vec![0; bucket_size],
        );
        let mut udp_socket = UdpSocket::new(udp_rx_buffer, udp_tx_buffer);
        let mut sockaddr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        {
            let sock = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
            sockaddr = sock.local_addr().unwrap();
        }
        udp_socket.bind(sockaddr).unwrap();

        // let mut sockets = SocketSet::new(vec![]);
        // let tcp_handle = sockets.add(udp_socket);

        workers.push(std::thread::spawn(move || {
            let now = Instant::now();
            let mut send_nbytes: usize = 0;
            for _ in 0..repeat {
                udp_socket
                    .send_slice(&bucket[..bucket_size], server_endpoint)
                    .unwrap();

                println!("bucket_size={}", bucket_size);
                send_nbytes += bucket_size;
            }

            println!(
                "now.elapsed().as_secs_f64()={}",
                now.elapsed().as_secs_f64()
            );
            let total_ngbs = send_nbytes as f64 / (1024. as f64).powf(3.);
            println!(
                "speed={}, it will be shutdown!",
                total_ngbs / now.elapsed().as_secs_f64()
            );
        }));
    }
    // multi_bar.listen();

    for worker in workers {
        worker.join().unwrap();
    }
}
