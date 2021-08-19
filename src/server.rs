pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::time::{Duration, Instant, SystemTime};


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
    let listener = TcpListener::bind(listen_address).unwrap();
    let sockaddr = listener.local_addr().unwrap();

    // let mut bucket: [u8; bucket_size] = [0; bucket_size];
    println!("Listening on {:?}", sockaddr);
    while match listener.accept() {
        Ok((mut stream, _)) => {
            // stream.set_nodelay(true).unwrap();
            // stream.set_nonblocking(true).unwrap();

            let mut bucket: Vec<u8> = vec![0; BUCKET_SIZE];
            workers.push(std::thread::spawn(move || {
                loop {
                    let mut target_nbytes = BUCKET_SIZE.to_be_bytes();
                    stream.read_exact(&mut target_nbytes[..]).unwrap();
                    // nonblocking_read_exact(&mut stream, &mut target_nbytes[..]).unwrap();
                    let target_nbytes = usize::from_be_bytes(target_nbytes);
                    if target_nbytes == 0 {
                        break
                    }
                    stream.read_exact(&mut bucket[..target_nbytes]).unwrap();
                    // nonblocking_read_exact(&mut stream, &mut bucket[..target_nbytes])
                    //     .unwrap();
                }
            }));

            true
        }
        Err(err) => {
            println!("listener.accept failed, err={:?}", err);
            false
        }
    } {}

    for worker in workers {
        worker.join().unwrap();
    }
}
