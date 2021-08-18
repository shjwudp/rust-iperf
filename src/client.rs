pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant, SystemTime};
use pbr::ProgressBar;


pub fn nonblocking_read_exact(
    stream: &mut std::net::TcpStream,
    mut buf: &mut [u8],
) -> io::Result<()> {
    while !buf.is_empty() {
        match stream.read(buf) {
            Ok(0) => break,
            Ok(n) => {
                let tmp = buf;
                buf = &mut tmp[n..];
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::Interrupted
                    || e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        std::thread::yield_now();
    }
    if !buf.is_empty() {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "failed to fill whole buffer",
        ))
    } else {
        Ok(())
    }
}

fn main() {
    let mut address = "127.0.0.1:63590".to_string();
    let mut num_of_socks = 1;
    const BUCKET_SIZE: usize = 20 * 1024 * 1024;
    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("tcp server.");
        ap.refer(&mut address)
            .add_option(&["--server_address"], Store, "Server address");
        ap.parse_args_or_exit();
    }

    let mut workers = Vec::new();
    for _ in 0..num_of_socks {
        let server_address = address.clone();
        let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
        workers.push(std::thread::spawn(move || {
            let work_type = WorkType::Recv;
            match TcpStream::connect(server_address.clone()) {
                Ok(mut stream) => {
                    stream.set_nodelay(true).unwrap();
                    stream.set_nonblocking(true).unwrap();

                    let now = Instant::now();
                    let mut recv_nbytes: usize = 0;
                    for _ in 0..10000 {
                        let mut target_nbytes = BUCKET_SIZE.to_be_bytes();
                        nonblocking_read_exact(&mut stream, &mut target_nbytes[..]).unwrap();
                        let target_nbytes = usize::from_be_bytes(target_nbytes);
                        nonblocking_read_exact(&mut stream, &mut bucket[..target_nbytes])
                            .unwrap();

                        recv_nbytes += target_nbytes;
                    }
                    println!("now.elapsed().as_secs_f64()={}", now.elapsed().as_secs_f64());
                    let total_ngbs = recv_nbytes as f64 / (1024. as f64).powf(3.);
                    println!(
                        "speed={}, it will be shutdown!",
                        total_ngbs / now.elapsed().as_secs_f64()
                    );
                }
                Err(err) => {
                    println!("Failed to connect: {}", err);
                }
            }
        }));
    }

    for worker in workers {
        worker.join().unwrap();
    }
}
