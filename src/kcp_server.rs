pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
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

    let socket = std::net::UdpSocket::bind(listen_address).unwrap();
    let sockaddr = socket.local_addr().unwrap();
    println!("Listening on {:?}", sockaddr);

    let socket = Arc::new(Mutex::new(socket));

    let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
    let (amt, src) = socket.lock().unwrap().recv_from(&mut bucket).unwrap();
    let mut kcp_handle = kcp::Kcp::new(
        0x11223344,
        KcpOutput {
            socket: socket.clone(),
            src,
        },
    );
    kcp_handle.set_nodelay(true, 10, 2, true);
    kcp_handle.set_rx_minrto(10);
    let kcp_handle = Arc::new(Mutex::new(kcp_handle));
    let kcp1 = kcp_handle.clone();
    workers.push(std::thread::spawn(move || {
        kcp1.lock().unwrap().input(&bucket[..amt]).unwrap();
        
        loop {
            let (amt, _) = socket.lock().unwrap().recv_from(&mut bucket).unwrap();
            kcp1.lock().unwrap().input(&bucket[..amt]).unwrap();
            std::thread::yield_now();
        }
    }));

    let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
    let kcp2 = kcp_handle.clone();
    workers.push(std::thread::spawn(move || {
        loop {
            let mut target_nbytes = BUCKET_SIZE.to_be_bytes();
            kcp2.lock().unwrap().recv(&mut target_nbytes[..]).unwrap();
            let target_nbytes = usize::from_be_bytes(target_nbytes);
            if target_nbytes == 0 {
                break
            }
            kcp2.lock().unwrap().recv(&mut bucket[..target_nbytes]).unwrap();

            std::thread::yield_now();
        }
    }));

    for worker in workers {
        worker.join().unwrap();
    }
}
