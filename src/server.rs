pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};

fn main() {
    let mut address = "0.0.0.0".to_string();
    let mut num_of_socks = 1;
    const bucket_size: usize = 2 * 1024 * 1024;
    {
        // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("tcp server.");
        ap.refer(&mut num_of_socks).add_option(
            &["-n", "--num_of_socks"],
            Store,
            "Number of sockets",
        );
        ap.refer(&mut address)
            .add_option(&["--address"], Store, "Listening address");
        ap.parse_args_or_exit();
    }
    println!("num_of_socks={}", num_of_socks);

    let mut workers = Vec::new();
    for _ in 0..num_of_socks {
        println!("hi there");
        let listen_address = address.clone();
        workers.push(std::thread::spawn(move || {
            println!("listen_address={:?}", listen_address);
            let listen_address = format!("{}:0", listen_address);
            // // let listen_to_address = format!("{}:0", *address);
            let listener = TcpListener::bind(listen_address).unwrap();
            let sockaddr = listener.local_addr().unwrap();
            let mut bucket: Vec<u8> = vec![0; bucket_size];

            // let mut bucket: [u8; bucket_size] = [0; bucket_size];
            println!("Listening on {:?}", sockaddr);
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        println!("New connection: {}", stream.peer_addr().unwrap());
                        let mut data = [0 as u8; 1024]; // using 50 byte buffer
                        let mut data = vec![0 as u8; bucket_size]; // using 10M byte buffer
                                                                   // stream.set_nonblocking(true).expect("set_nonblocking call failed");

                        let target = PerfRequest {
                            work_type: WorkType::Send,
                        };
                        let mut encoded: Vec<u8> = bincode::serialize(&target).unwrap();
                        stream.read_exact(&mut encoded[..]).unwrap();

                        let request: PerfRequest = bincode::deserialize(&encoded[..]).unwrap();
                        println!("request={:?}", request);
                        let mut total_nbytes = 0;
                        loop {
                            let nbytes = match request.work_type {
                                WorkType::Send => {
                                    match stream.write(&data[..]) {
                                        Ok(nbytes) => {
                                            if nbytes == 0 {
                                                println!("stream.write 0 bytes, exit directly");
                                                break
                                            }
                                            nbytes
                                        },
                                        Err(err) => {
                                            println!("stream.write failed, err={:?}", err);
                                            break
                                        },
                                    }
                                },
                                _ => {
                                    match stream.read(&mut data[..]) {
                                        Ok(nbytes) => {
                                            if nbytes == 0 {
                                                println!("stream.read 0 bytes, exit directly");
                                                break
                                            }
                                            nbytes
                                        },
                                        Err(err) => {
                                            println!("stream.write failed, err={:?}", err);
                                            break
                                        },
                                    }
                                },
                            };
                            total_nbytes += nbytes;
                            // println!("total_nbytes={}", total_nbytes);
                        };
                        println!("Ready to disconnect, total_nbytes={}", total_nbytes);
                    }
                    Err(err) => {
                        println!("Error: {}", err);
                    }
                }
            }
        }));
    }

    for worker in workers {
        worker.join().unwrap();
    }
}
