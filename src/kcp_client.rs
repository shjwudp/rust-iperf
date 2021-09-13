pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use pbr::{MultiBar, ProgressBar};
use socket2::{Domain, Socket, Type};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct KcpOutput {
    socket: Arc<std::net::UdpSocket>,
}

impl Write for KcpOutput {
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

    // let multi_bar = MultiBar::new();
    let mut workers = Vec::new();
    for _ in 0..nstreams {
        let mut bucket: Vec<u8> = vec![0; bucket_size];
        // let mut progress = multi_bar.create_bar(repeat);

        let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
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
        let server_sockaddr: SocketAddr = address.parse().expect(&format!("address={}", address));
        socket.connect(&server_sockaddr.into()).unwrap();
        socket.set_send_buffer_size(4194304).unwrap();
        socket.set_recv_buffer_size(4194304).unwrap();
        println!(
            "send_buffer_size={}, recv_buffer_size={}",
            socket.send_buffer_size().unwrap(),
            socket.recv_buffer_size().unwrap()
        );
        let socket: std::net::UdpSocket = socket.into();
        let socket = Arc::new(socket);

        let mut kcp_handle = kcp::Kcp::new(
            0x11223344,
            KcpOutput {
                socket: socket.clone(),
            },
        );
        kcp_handle.set_wndsize(65535, 65535);
        kcp_handle.set_nodelay(true, 10, 2, true);
        kcp_handle.set_fast_resend(1);
        kcp_handle.set_mtu(bucket_size * 2).unwrap();

        socket.set_nonblocking(true).unwrap();

        let mut log_count = 0;
        workers.push(std::thread::spawn(move || {
            let now = Instant::now();
            let mut send_nbytes: usize = 0;
            for _ in 0..repeat {
                kcp_handle
                    .update(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u32,
                    )
                    .unwrap();

                let send_size = kcp_handle.send(&bucket[..bucket_size]).unwrap();
                send_nbytes += send_size;

                loop {
                    kcp_handle
                        .update(
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u32,
                        )
                        .unwrap();

                    match socket.recv_from(&mut bucket[..]) {
                        Ok((recv_nbytes, _)) => {
                            // println!("recv it, recv_nbytes={}", recv_nbytes);
                            kcp_handle.input(&bucket[..recv_nbytes]).unwrap();
                            kcp_handle.recv(&mut bucket[..]);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if kcp_handle.wait_snd() < 1024 {
                                break;
                            }
                            continue;
                        },
                        Err(e) => panic!("Can't recv_from, err={:?}", e),
                    }
                }

                // let (recv_nbytes, src) = socket.recv_from(&mut bucket[..]);
                // kcp_handle.input(&bucket[..recv_nbytes]).unwrap();
                // // kcp_handle.flush().unwrap();
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
