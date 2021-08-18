pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant, SystemTime};
use pbr::ProgressBar;

pub fn nonblocking_write_all(stream: &mut std::net::TcpStream, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        match stream.write(buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => buf = &buf[n..],
            Err(ref e)
                if e.kind() == io::ErrorKind::Interrupted
                    || e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        std::thread::yield_now();
    }
    Ok(())
}

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
    let mut bucket_size: usize = 1 * (1024 as usize).pow(2);
    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("tcp server.");
        ap.refer(&mut address)
            .add_option(&["--server_address"], Store, "Server address");
        ap.refer(&mut bucket_size)
            .add_option(&["--bucket_size"], Store, "Bucket size");
        ap.parse_args_or_exit();
    }

    let mut workers = Vec::new();
    for _ in 0..num_of_socks {
        let server_address = address.clone();
        let mut bucket: Vec<u8> = vec![0; bucket_size];
        workers.push(std::thread::spawn(move || {
            let work_type = WorkType::Recv;
            match TcpStream::connect(server_address.clone()) {
                Ok(mut stream) => {
                    stream.set_nodelay(true).unwrap();
                    stream.set_nonblocking(true).unwrap();

                    let repeat = 10000;
                    let mut pb = ProgressBar::new(repeat);

                    let now = Instant::now();
                    let mut send_nbytes: usize = 0;
                    for _ in 0..10000 {
                        let target_nbytes = bucket_size.to_be_bytes();
                        nonblocking_write_all(&mut stream, &target_nbytes[..]).unwrap();
                        nonblocking_write_all(&mut stream, &bucket[..bucket_size]).unwrap();

                        send_nbytes += bucket_size;
                        pb.inc();
                    }
                    pb.finish();
                    println!("now.elapsed().as_secs_f64()={}", now.elapsed().as_secs_f64());
                    let total_ngbs = send_nbytes as f64 / (1024. as f64).powf(3.);
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
