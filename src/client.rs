pub mod proto;

use crate::proto::{PerfRequest, WorkType};
use argparse::{ArgumentParser, Store, StoreTrue};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant, SystemTime};
use pbr::ProgressBar;

fn main() {
    let mut address = "127.0.0.1:63590".to_string();
    let mut num_of_socks = 1;
    const bucket_size: usize = 1 * 1024 * 1024;
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
        let bucket: Vec<u8> = vec![0; bucket_size];
        workers.push(std::thread::spawn(move || {
            let work_type = WorkType::Recv;
            match TcpStream::connect(server_address.clone()) {
                Ok(mut tcp_stream) => {
                    println!("Successfully connected to server in {}", server_address);

                    let msg: Vec<u8> = bincode::serialize(&PerfRequest {
                        work_type: work_type.clone(),
                    })
                    .unwrap();
                    let nbytes = tcp_stream.write(&msg).unwrap();
                    println!("Sent nbytes={}, awaiting reply...", nbytes);

                    let now = Instant::now();
                    // let mut data = [0 as u8; 6]; // using 6 byte buffer
                    let mut data = vec![0 as u8; bucket_size];
                    let mut total_nbytes: u64 = 0;
                    let target_nbytes = 10 * (1024 as u64).pow(3);
                    let pbr_count = 1000;
                    let pbr_unit = target_nbytes as f64 / pbr_count as f64;
                    let mut pbr_id = 0;
                    let mut pb = ProgressBar::new(pbr_count);
                    for _ in 0..target_nbytes {
                        let nbytes = match work_type {
                            WorkType::Send => tcp_stream.read(&mut data[..]).unwrap(),
                            _ => tcp_stream.write(&data[..]).unwrap(),
                        } as u64;

                        total_nbytes += nbytes;
                        while pbr_id < pbr_count && pbr_id as f64 * pbr_unit <= total_nbytes as f64 {
                            pb.inc();
                            pbr_id += 1;
                        }

                        if total_nbytes >= target_nbytes {
                            break;
                        }
                    }
                    pb.finish();
                    println!("now.elapsed().as_secs_f64()={}", now.elapsed().as_secs_f64());
                    let total_ngbs = total_nbytes as f64 / (1024. as f64).powf(3.);
                    println!(
                        "speed={}, it will be shutdown!",
                        total_ngbs / now.elapsed().as_secs_f64()
                    );
                    tcp_stream.shutdown(Shutdown::Both).unwrap();
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
