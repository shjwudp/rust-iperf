pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use socket2::{Domain, Socket, Type};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

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
    const BUCKET_SIZE: usize = 1 * (1024 as usize).pow(3);
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

    let addr: SocketAddr = listen_address.parse().unwrap();
    let socket = Socket::new(
        match addr {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        },
        Type::DGRAM,
        None,
    )
    .unwrap();
    socket.bind(&addr.into()).unwrap();
    println!(
        "send_buffer_size={}, recv_buffer_size={}",
        socket.send_buffer_size().unwrap(),
        socket.recv_buffer_size().unwrap()
    );
    socket.set_send_buffer_size(4194304).unwrap();
    socket.set_recv_buffer_size(4194304).unwrap();

    let socket: std::net::UdpSocket = socket.into();
    let sockaddr = socket.local_addr().unwrap();
    println!("Listening on {:?}", sockaddr);

    let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];

    let socket = Arc::new(socket);
    // let socket = Arc::new(Mutex::new(socket));
    // let mut kcp_handle = kcp::Kcp::new_stream(
    //     0x11223344,
    //     KcpOutput {
    //         socket: socket.clone(),
    //         src,
    //     },
    // );
    // kcp_handle.set_nodelay(true, 10, 2, true);
    // kcp_handle.set_rx_minrto(10);
    // let kcp_handle = Arc::new(Mutex::new(kcp_handle));
    // let kcp1 = kcp_handle.clone();
    // workers.push(std::thread::spawn(move || {
    //     kcp1.lock().unwrap().input(&bucket[..amt]).unwrap();
    //     loop {
    //         let (amt, _) = socket.lock().unwrap().recv_from(&mut bucket).unwrap();
    //         kcp1.lock().unwrap().input(&bucket[..amt]).unwrap();
    //         std::thread::yield_now();
    //     }
    // }));

    // let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
    // let kcp2 = kcp_handle.clone();

    workers.push(std::thread::spawn(move || loop {
        let (_, src) = socket.peek_from(&mut bucket).unwrap();
        let mut socket = KcpOutput {
            socket: socket.clone(),
            src,
        };
        loop {
            let mut target_nbytes = BUCKET_SIZE.to_be_bytes();
            socket.read_exact(&mut target_nbytes[..]).unwrap();
            // kcp2.lock().unwrap().recv(&mut target_nbytes[..]).unwrap();
            let target_nbytes = usize::from_be_bytes(target_nbytes);
            if target_nbytes == 0 {
                break;
            }
            socket.read_exact(&mut bucket[..target_nbytes]).unwrap();
        }
        // kcp2.lock()
        //     .unwrap()
        //     .recv(&mut bucket[..target_nbytes])
        //     .unwrap();

        // std::thread::yield_now();
    }));

    for worker in workers {
        worker.join().unwrap();
    }
}
