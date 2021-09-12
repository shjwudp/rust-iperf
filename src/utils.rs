use nix::net::if_::InterfaceFlags;
use nix::sys::socket::{AddressFamily, InetAddr, IpAddr, SockAddr};
use smoltcp::wire::{IpAddress, IpCidr};
use std::fs;
use std::io;
use std::io::{Read, Write};

pub fn get_net_if_speed(device: &str) -> i32 {
    const DEFAULT_SPEED: i32 = 10000;

    let speed_path = format!("/sys/class/net/{}/speed", device);
    match fs::read_to_string(speed_path.clone()) {
        Ok(speed_str) => {
            return speed_str.trim().parse().unwrap_or(DEFAULT_SPEED);
        }
        Err(_) => {
            tracing::debug!(
                "Could not get speed from {}. Defaulting to 10 Gbps.",
                speed_path
            );
            DEFAULT_SPEED
        }
    }
}

#[derive(Debug, Clone)]
pub struct NCCLSocketDev {
    pub interface_name: String,
    pub addr: SockAddr,
    pub pci_path: String,
    pub ip_cidr: IpCidr,
}

pub fn find_interfaces() -> Vec<NCCLSocketDev> {
    let nccl_socket_family = std::env::var("NCCL_SOCKET_FAMILY")
        .unwrap_or("-1".to_string())
        .parse::<i32>()
        .unwrap_or(-1);
    let nccl_socket_ifname =
        std::env::var("NCCL_SOCKET_IFNAME").unwrap_or("^docker,lo".to_string());
    // TODO @shjwudp: support parse sockaddr from NCCL_COMM_ID

    let mut search_not = Vec::<&str>::new();
    let mut search_exact = Vec::<&str>::new();
    if nccl_socket_ifname.starts_with("^") {
        search_not = nccl_socket_ifname[1..].split(",").collect();
    } else if nccl_socket_ifname.starts_with("=") {
        search_exact = nccl_socket_ifname[1..].split(",").collect();
    } else {
        search_exact = nccl_socket_ifname.split(",").collect();
    }

    let mut socket_devs = Vec::<NCCLSocketDev>::new();
    const MAX_IF_NAME_SIZE: usize = 16;
    let addrs = nix::ifaddrs::getifaddrs().unwrap();
    for ifaddr in addrs {
        match ifaddr.address {
            Some(addr) => {
                if addr.family() != AddressFamily::Inet && addr.family() != AddressFamily::Inet6 {
                    continue;
                }
                if ifaddr.flags.contains(InterfaceFlags::IFF_LOOPBACK) {
                    continue;
                }

                assert_eq!(ifaddr.interface_name.len() < MAX_IF_NAME_SIZE, true);
                let found_ifs: Vec<&NCCLSocketDev> = socket_devs
                    .iter()
                    .filter(|scoket_dev| scoket_dev.interface_name == ifaddr.interface_name)
                    .collect();
                if found_ifs.len() > 0 {
                    continue;
                }

                let pci_path = format!("/sys/class/net/{}/device", ifaddr.interface_name);
                let pci_path: String = match std::fs::canonicalize(pci_path) {
                    Ok(pci_path) => pci_path.to_str().unwrap_or("").to_string(),
                    Err(_) => "".to_string(),
                };

                socket_devs.push(NCCLSocketDev {
                    addr: addr,
                    interface_name: ifaddr.interface_name.clone(),
                    pci_path: pci_path,
                    ip_cidr: nix_if_addr_to_cidr(&ifaddr),
                })
            }
            None => {
                tracing::warn!(
                    "interface {} with unsupported address family",
                    ifaddr.interface_name
                );
            }
        }
    }

    let search_not = &mut search_not;
    let search_exact = &mut search_exact;
    let socket_devs = socket_devs
        .iter()
        .filter({
            |socket_dev| -> bool {
                let (sockaddr, _) = socket_dev.addr.as_ffi_pair();
                if nccl_socket_family != -1 && sockaddr.sa_family as i32 != nccl_socket_family {
                    return false;
                }

                for not_interface in &*search_not {
                    if socket_dev.interface_name.starts_with(not_interface) {
                        return false;
                    }
                }
                if !(&*search_exact).is_empty() {
                    let mut ok = false;
                    for exact_interface in &*search_exact {
                        if socket_dev.interface_name.starts_with(exact_interface) {
                            ok = true;
                            break;
                        }
                    }
                    if !ok {
                        return false;
                    }
                }

                return true;
            }
        })
        .cloned()
        .collect();

    socket_devs
}

pub fn nix_if_addr_to_cidr(if_addr: &nix::ifaddrs::InterfaceAddress) -> IpCidr {
    let address = match if_addr.address.unwrap() {
        SockAddr::Inet(x) => x,
        _ => panic!("{:?}", if_addr),
    }
    .ip();
    let netmask = match if_addr.netmask.unwrap() {
        SockAddr::Inet(x) => x,
        _ => panic!("{:?}", if_addr),
    }
    .ip();

    let ip_addr = match address {
        IpAddr::V4(v4) => {
            let ip_arr = v4.octets();
            IpAddress::v4(ip_arr[0], ip_arr[1], ip_arr[2], ip_arr[3])
        }
        IpAddr::V6(v6) => {
            let ip_arr = v6.segments();
            IpAddress::v6(
                ip_arr[0], ip_arr[1], ip_arr[2], ip_arr[3], ip_arr[4], ip_arr[5], ip_arr[6],
                ip_arr[7],
            )
        }
    };
    let netmask: u32 = match netmask {
        IpAddr::V4(v4) => {
            v4.octets().iter().map(|x| x.count_ones()).sum()
        }
        IpAddr::V6(v6) => {
            v6.segments().iter().map(|x| x.count_ones()).sum()
        }
    };

    IpCidr::new(ip_addr, netmask as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let sockdevs = find_interfaces();
        println!("{:?}", sockdevs);
    }
}
