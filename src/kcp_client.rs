pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant, SystemTime};
use pbr::{ProgressBar, MultiBar};
use std::sync::{Arc, Mutex};


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
    let mut address = "127.0.0.1:63590".to_string();
    let mut bucket_size: usize = 128;
    let mut repeat = 1000000;
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

    let multi_bar = MultiBar::new();
    let mut workers = Vec::new();
    for _ in 0..nstreams {
        let bucket: Vec<u8> = vec![0; bucket_size];
        let mut progress = multi_bar.create_bar(repeat);

        let socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.connect(address.clone()).unwrap();
        let server_sockaddr = socket.peer_addr().unwrap();
        let socket = Arc::new(Mutex::new(socket));

        let mut kcp_handle = kcp::Kcp::new(
            0x11223344,
            KcpOutput {
                socket: socket.clone(),
                src: server_sockaddr,
            },
        );
        kcp_handle.set_nodelay(true, 10, 2, true);
        kcp_handle.set_fast_resend(1);

        workers.push(std::thread::spawn(move || {
            let now = Instant::now();
            let mut send_nbytes: usize = 0;
            for _ in 0..repeat {
                let target_nbytes = bucket_size.to_be_bytes();

                kcp_handle.send(&target_nbytes[..]).unwrap();
                kcp_handle.send(&bucket[..bucket_size]).unwrap();

                send_nbytes += bucket_size;
                progress.inc();
            }
            progress.finish();

            kcp_handle.send(&(0 as usize).to_be_bytes()[..]).unwrap();

            println!("now.elapsed().as_secs_f64()={}", now.elapsed().as_secs_f64());
            let total_ngbs = send_nbytes as f64 / (1024. as f64).powf(3.);
            println!(
                "speed={}, it will be shutdown!",
                total_ngbs / now.elapsed().as_secs_f64()
            );
        }))
    }
    multi_bar.listen();

    for worker in workers {
        worker.join().unwrap();
    }
}
