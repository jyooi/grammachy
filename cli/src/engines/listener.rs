//! The loopback listener a transient user unit owns.
//!
//! A local-server engine must never post a Selection to a port that another
//! process bound. So the adapter never trusts a port number. Before every
//! request it asks systemd whether the unit is active and which control group
//! it runs in. It reads the processes of that group.
//!
//! Then it reads which loopback socket in `LISTEN` state one of those
//! processes holds. That socket is the endpoint. No socket, no request.
//!
//! The machine is read through `systemctl --user show`, `/sys/fs/cgroup`,
//! `/proc/<pid>/fd`, and `/proc/net/tcp` plus `tcp6`. Every parser here is a
//! pure function of the text it reads, so the tests hand in text.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::process::Command;

/// What the unit owns right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owned {
    /// The unit is active and this is its loopback listener.
    Listening(SocketAddr),
    /// The unit is active and its loopback listener is not open yet.
    Starting,
    /// The unit is not active, so nothing on the machine speaks for it.
    Inactive,
    /// systemd could not be asked.
    Unknown(String),
}

/// One listening TCP socket on the loopback interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listener {
    pub address: SocketAddr,
    pub inode: u64,
}

/// What `systemctl show` said about the unit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitFacts {
    pub active_state: String,
    pub main_pid: u32,
    pub control_group: String,
}

/// The `LISTEN` state in `/proc/net/tcp`.
const LISTEN: &str = "0A";

/// Where the unified cgroup hierarchy is mounted.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Read the machine and answer what the unit owns.
pub fn owned_listener(unit: &str) -> Owned {
    let facts = match unit_facts(unit) {
        Ok(facts) => facts,
        Err(why) => return Owned::Unknown(why),
    };
    if !is_active(&facts.active_state) {
        return Owned::Inactive;
    }

    let pids = unit_pids(&facts);
    let inodes: HashSet<u64> = pids.iter().flat_map(|pid| socket_inodes(*pid)).collect();
    let listeners = loopback_listeners();
    match owned_by(&listeners, &inodes) {
        Some(address) => Owned::Listening(address),
        None => Owned::Starting,
    }
}

/// Ask systemd about one unit.
fn unit_facts(unit: &str) -> Result<UnitFacts, String> {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            unit,
            "--property=ActiveState,MainPID,ControlGroup",
        ])
        .output()
        .map_err(|error| format!("systemctl --user could not run: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "systemctl --user could not show {unit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(parse_show(&String::from_utf8_lossy(&output.stdout)))
}

/// The `Key=Value` lines `systemctl show` prints.
pub fn parse_show(text: &str) -> UnitFacts {
    let mut facts = UnitFacts::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "ActiveState" => facts.active_state = value.trim().to_string(),
            "MainPID" => facts.main_pid = value.trim().parse().unwrap_or(0),
            "ControlGroup" => facts.control_group = value.trim().to_string(),
            _ => {}
        }
    }
    facts
}

pub fn is_active(state: &str) -> bool {
    matches!(state, "active" | "activating" | "reloading")
}

/// The processes of the unit: its control group, or its main process alone
/// when the group cannot be read.
fn unit_pids(facts: &UnitFacts) -> Vec<u32> {
    if !facts.control_group.is_empty() {
        let procs = Path::new(CGROUP_ROOT)
            .join(facts.control_group.trim_start_matches('/'))
            .join("cgroup.procs");
        if let Ok(text) = std::fs::read_to_string(procs) {
            let pids = parse_pids(&text);
            if !pids.is_empty() {
                return pids;
            }
        }
    }
    if facts.main_pid > 0 {
        vec![facts.main_pid]
    } else {
        Vec::new()
    }
}

/// One pid per line, as `cgroup.procs` prints them.
pub fn parse_pids(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// The socket inodes one process holds open.
fn socket_inodes(pid: u32) -> Vec<u64> {
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter_map(|target| socket_inode(&target.to_string_lossy()))
        .collect()
}

/// The inode in a `socket:[12345]` link target.
pub fn socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Every listening loopback socket on the machine, both address families.
fn loopback_listeners() -> Vec<Listener> {
    let mut listeners = Vec::new();
    if let Ok(text) = std::fs::read_to_string("/proc/net/tcp") {
        listeners.extend(parse_proc_net_tcp(&text));
    }
    if let Ok(text) = std::fs::read_to_string("/proc/net/tcp6") {
        listeners.extend(parse_proc_net_tcp(&text));
    }
    listeners
}

/// The listening loopback sockets of one `/proc/net/tcp` or `tcp6` table.
///
/// Each row is `sl local_address rem_address st ... uid timeout inode`. The
/// address is hex in the byte order of the machine. This binary ships for
/// little-endian machines only, so the hex is read as little-endian bytes.
pub fn parse_proc_net_tcp(text: &str) -> Vec<Listener> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 || fields[3] != LISTEN {
                return None;
            }
            let address = parse_local_address(fields[1])?;
            let inode = fields[9].parse().ok()?;
            address
                .ip()
                .is_loopback()
                .then_some(Listener { address, inode })
        })
        .collect()
}

/// `0100007F:1F91` or its 32 hex character IPv6 form, as a socket address.
///
/// An IPv4-mapped IPv6 address answers as the IPv4 one, so the URL the
/// adapter builds is the plain `127.0.0.1:port`.
fn parse_local_address(field: &str) -> Option<SocketAddr> {
    let (hex, port) = field.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match hex.len() {
        8 => {
            let word = u32::from_str_radix(hex, 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(word.to_le_bytes()))
        }
        32 => {
            let mut bytes = [0u8; 16];
            for (index, chunk) in bytes.chunks_mut(4).enumerate() {
                let word = u32::from_str_radix(&hex[index * 8..index * 8 + 8], 16).ok()?;
                chunk.copy_from_slice(&word.to_le_bytes());
            }
            let v6 = Ipv6Addr::from(bytes);
            match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6(v6),
            }
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

/// The first listener whose socket one of the unit's processes holds.
pub fn owned_by(listeners: &[Listener], inodes: &HashSet<u64>) -> Option<SocketAddr> {
    listeners
        .iter()
        .find(|listener| inodes.contains(&listener.inode))
        .map(|listener| listener.address)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1F91 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 10811 1 0000000000000000 100 0 0 10 0\n\
   2: 0100007F:A8C2 0100007F:1F91 01 00000000:00000000 00:00000000 00000000  1000        0 55556 1 0000000000000000 100 0 0 10 0\n";

    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0000000000000000FFFF00000100007F:1F91 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 77777 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000000000000000000001000000:1F92 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 77778 1 0000000000000000 100 0 0 10 0\n\
   2: 00000000000000000000000000000000:0050 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 77779 1 0000000000000000 100 0 0 10 0\n";

    #[test]
    fn only_listening_loopback_rows_are_read() {
        let listeners = parse_proc_net_tcp(TCP);

        assert_eq!(
            listeners,
            [Listener {
                address: "127.0.0.1:8081".parse().unwrap(),
                inode: 55555
            }]
        );
    }

    #[test]
    fn a_mapped_ipv4_listener_answers_as_ipv4_and_ipv6_loopback_as_itself() {
        let listeners = parse_proc_net_tcp(TCP6);

        assert_eq!(
            listeners,
            [
                Listener {
                    address: "127.0.0.1:8081".parse().unwrap(),
                    inode: 77777
                },
                Listener {
                    address: "[::1]:8082".parse().unwrap(),
                    inode: 77778
                },
            ]
        );
    }

    #[test]
    fn the_listener_a_unit_process_holds_is_the_owned_one() {
        let listeners = parse_proc_net_tcp(TCP);
        let held: HashSet<u64> = [55555, 55556].into_iter().collect();
        let other: HashSet<u64> = [10811].into_iter().collect();

        assert_eq!(
            owned_by(&listeners, &held),
            Some("127.0.0.1:8081".parse().unwrap())
        );
        assert_eq!(owned_by(&listeners, &other), None);
    }

    #[test]
    fn the_show_output_is_read_by_key() {
        let facts = parse_show(
            "ActiveState=active\nMainPID=4242\nControlGroup=/user.slice/user-1000.slice/user@1000.service/app.slice/grammachy-languagetool.service\n",
        );

        assert_eq!(facts.active_state, "active");
        assert_eq!(facts.main_pid, 4242);
        assert!(facts
            .control_group
            .ends_with("grammachy-languagetool.service"));
        assert!(is_active(&facts.active_state));
        assert!(!is_active("inactive"));
    }

    #[test]
    fn socket_links_and_pid_lists_are_read() {
        assert_eq!(socket_inode("socket:[55555]"), Some(55555));
        assert_eq!(socket_inode("/dev/null"), None);
        assert_eq!(parse_pids("4242\n4243\n\n"), [4242, 4243]);
    }
}
