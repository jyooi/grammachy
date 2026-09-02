//! The loopback listener a transient user unit owns, and the proof that one
//! connection reached it.
//!
//! A local-server engine must never post a Selection to a port that another
//! process bound. So the adapter never trusts a port number. Before every
//! request it asks systemd four things about the unit. Is it active? Which
//! process is its main one? Is it transient? What command does it run?
//!
//! Then it reads which loopback socket in `LISTEN` state that main process
//! holds on the port the unit's own command line names. One process may hold
//! more than one listener, so the port is what picks the server. That socket
//! is the endpoint. No socket, no request.
//!
//! A listener read before the request is not the socket the request uses.
//! So the second half is [`accepted_by`]. Once the adapter is connected, it
//! reads the server end of that very connection from `/proc/net/tcp`. Then
//! it proves that the same main process holds that end. The request goes
//! over the connection only after that proof.
//!
//! So nothing that takes the port between the two reads can receive the
//! text. What this cannot prove is that no other program of the same user
//! started the unit under this name with the same command. A program that
//! runs as the user already reads the primary selection and the clipboard
//! directly. That is outside what a loopback check can protect.
//!
//! The machine is read through `systemctl --user show`, `/proc/<pid>/fd`, and
//! `/proc/net/tcp` plus `tcp6`. Every parser here is a pure function of the
//! text it reads, so the tests hand in text.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// What the unit owns right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owned {
    /// The unit is active and its main process holds this loopback listener.
    Listening(Peer),
    /// The unit is active and its loopback listener is not open yet.
    Starting,
    /// The unit is active, and either its command line or the listener it
    /// holds is not one this plugin runs. A command line with no `--port`
    /// answers this before any socket is read. The message says which fact
    /// refused it.
    Foreign(String),
    /// The unit is not active, so nothing on the machine speaks for it.
    Inactive,
    /// systemd could not be asked.
    Unknown(String),
}

/// The listener one unit's main process holds, and how the unit was started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub address: SocketAddr,
    /// The main process of the unit, which must hold every socket the
    /// adapter talks to.
    pub pid: u32,
    /// Whether `systemd-run` made the unit, rather than a unit file.
    pub transient: bool,
    pub exec_start: ExecStart,
}

/// The `path=` and `argv[]=` of the `ExecStart` line `systemctl show` prints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecStart {
    pub path: String,
    /// The argument vector as one space-joined line, the way systemd prints
    /// it, with the program as its first word.
    pub argv: String,
}

/// One TCP socket on the loopback interface, as `/proc/net/tcp` lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Socket {
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub listening: bool,
    pub inode: u64,
}

/// What `systemctl show` said about the unit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitFacts {
    pub active_state: String,
    pub main_pid: u32,
    pub transient: bool,
    pub exec_start: ExecStart,
}

/// The `LISTEN` state in `/proc/net/tcp`.
const LISTEN: &str = "0A";

/// The `ESTABLISHED` state in `/proc/net/tcp`.
const ESTABLISHED: &str = "01";

/// How long [`accepted_by`] waits for the server to accept the connection.
///
/// A connection the server did not accept yet sits in its queue with no
/// inode, so the proof has to wait for the `accept`. A JVM under load takes
/// milliseconds. This is far above that and far below the Check timeout.
const ACCEPT_BUDGET: Duration = Duration::from_secs(2);

/// Time between two reads while the server accepts.
const ACCEPT_PROBE: Duration = Duration::from_millis(10);

/// Read the machine and answer what the unit owns.
pub fn owned_listener(unit: &str) -> Owned {
    let facts = match unit_facts(unit) {
        Ok(facts) => facts,
        Err(why) => return Owned::Unknown(why),
    };
    if !is_active(&facts.active_state) {
        return Owned::Inactive;
    }
    if facts.main_pid == 0 {
        return Owned::Starting;
    }

    let Some(port) = port_of(&facts.exec_start.argv) else {
        return Owned::Foreign("the unit's command line names no --port".to_string());
    };

    let inodes = socket_inodes(facts.main_pid);
    let sockets = loopback_sockets();
    match owned_by(&sockets, &inodes, port) {
        Some(address) => Owned::Listening(Peer {
            address,
            pid: facts.main_pid,
            transient: facts.transient,
            exec_start: facts.exec_start,
        }),
        None => match held_ports(&sockets, &inodes) {
            held if held.is_empty() => Owned::Starting,
            held => Owned::Foreign(format!(
                "the unit's command line names port {port}, and it listens on {}",
                held.iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
    }
}

/// Every loopback port the holder of `inodes` listens on.
///
/// A unit that holds none is still starting. A unit that holds some, but not
/// the one its own command line names, is not one this plugin started.
pub fn held_ports(sockets: &[Socket], inodes: &HashSet<u64>) -> Vec<u16> {
    sockets
        .iter()
        .filter(|socket| socket.listening && inodes.contains(&socket.inode))
        .map(|socket| socket.local.port())
        .collect()
}

/// Prove that the server end of `stream` is a socket the process `pid` holds.
///
/// The server end is the `ESTABLISHED` row whose local address is the one the
/// stream connected to and whose remote address is the stream's own. Its
/// inode is the file the server process holds. The check waits for the server
/// to accept, because the row carries no inode before that.
pub fn accepted_by(stream: &TcpStream, pid: u32) -> Result<(), String> {
    let client = stream
        .local_addr()
        .map_err(|error| format!("the connection has no local address: {error}"))?;
    let server = stream
        .peer_addr()
        .map_err(|error| format!("the connection has no peer address: {error}"))?;

    let deadline = Instant::now() + ACCEPT_BUDGET;
    loop {
        if let Some(inode) = accepted_inode(&loopback_sockets(), server, client) {
            return if socket_inodes(pid).contains(&inode) {
                Ok(())
            } else {
                Err(format!(
                    "the connection to {server} was accepted by a process other than {pid}, the main process of the unit"
                ))
            };
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the connection to {server} was not accepted within {} s",
                ACCEPT_BUDGET.as_secs()
            ));
        }
        sleep(ACCEPT_PROBE);
    }
}

/// Ask systemd about one unit.
fn unit_facts(unit: &str) -> Result<UnitFacts, String> {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            unit,
            "--property=ActiveState,MainPID,Transient,ExecStart",
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
            "Transient" => facts.transient = value.trim() == "yes",
            "ExecStart" => facts.exec_start = parse_exec_start(value),
            _ => {}
        }
    }
    facts
}

/// The `path=` and `argv[]=` fields of one `ExecStart` value.
///
/// systemd prints `{ path=/usr/bin/x ; argv[]=/usr/bin/x --flag ; ... }`.
/// Each field ends at the next ` ; `, so a path with a space survives.
pub fn parse_exec_start(value: &str) -> ExecStart {
    let field = |name: &str| -> String {
        let Some(start) = value.find(name) else {
            return String::new();
        };
        let rest = &value[start + name.len()..];
        let end = rest.find(" ; ").unwrap_or(rest.len());
        rest[..end].trim_end_matches(" }").trim().to_string()
    };
    ExecStart {
        path: field("path="),
        argv: field("argv[]="),
    }
}

pub fn is_active(state: &str) -> bool {
    matches!(state, "active" | "activating" | "reloading")
}

/// The socket inodes one process holds open.
fn socket_inodes(pid: u32) -> HashSet<u64> {
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return HashSet::new();
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

/// Every loopback TCP socket on the machine, both address families.
fn loopback_sockets() -> Vec<Socket> {
    let mut sockets = Vec::new();
    if let Ok(text) = std::fs::read_to_string("/proc/net/tcp") {
        sockets.extend(parse_proc_net_tcp(&text));
    }
    if let Ok(text) = std::fs::read_to_string("/proc/net/tcp6") {
        sockets.extend(parse_proc_net_tcp(&text));
    }
    sockets
}

/// The loopback sockets of one `/proc/net/tcp` or `tcp6` table, in the
/// `LISTEN` and `ESTABLISHED` states.
///
/// Each row is `sl local_address rem_address st ... uid timeout inode`. The
/// address is hex in the byte order of the machine. This binary ships for
/// little-endian machines only, so the hex is read as little-endian bytes.
pub fn parse_proc_net_tcp(text: &str) -> Vec<Socket> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return None;
            }
            let listening = match fields[3] {
                LISTEN => true,
                ESTABLISHED => false,
                _ => return None,
            };
            let local = parse_address(fields[1])?;
            let remote = parse_address(fields[2])?;
            let inode = fields[9].parse().ok()?;
            local.ip().is_loopback().then_some(Socket {
                local,
                remote,
                listening,
                inode,
            })
        })
        .collect()
}

/// `0100007F:1F91` or its 32 hex character IPv6 form, as a socket address.
///
/// An IPv4-mapped IPv6 address answers as the IPv4 one, so the URL the
/// adapter builds is the plain `127.0.0.1:port`.
fn parse_address(field: &str) -> Option<SocketAddr> {
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

/// The listener on `port` whose socket is one of `inodes`.
///
/// The port comes from the unit's own command line, because one process may
/// hold more than one loopback listener. A JVM with a debug agent holds two,
/// and the first row of `/proc/net/tcp` is then the wrong one.
pub fn owned_by(sockets: &[Socket], inodes: &HashSet<u64>, port: u16) -> Option<SocketAddr> {
    sockets
        .iter()
        .find(|socket| {
            socket.listening && socket.local.port() == port && inodes.contains(&socket.inode)
        })
        .map(|socket| socket.local)
}

/// The value after `--port` on one command line. A port holds no space, so
/// the word after the flag is the whole value.
///
/// This is the one parser of the port. [`owned_listener`] reads it to find
/// the right listener, and `unit::launched_here` reads it to prove the shape.
pub fn port_of(argv: &str) -> Option<u16> {
    let mut words = argv.split(' ');
    while let Some(word) = words.next() {
        if word == "--port" {
            return words.next()?.parse().ok();
        }
    }
    None
}

/// The inode of the server end of one accepted connection, or `None` while
/// the connection waits in the accept queue.
pub fn accepted_inode(sockets: &[Socket], server: SocketAddr, client: SocketAddr) -> Option<u64> {
    sockets
        .iter()
        .find(|socket| !socket.listening && socket.local == server && socket.remote == client)
        .map(|socket| socket.inode)
        .filter(|inode| *inode != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1F91 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 10811 1 0000000000000000 100 0 0 10 0\n\
   2: 0100007F:1F91 0100007F:A8C2 01 00000000:00000000 00:00000000 00000000  1000        0 55557 1 0000000000000000 100 0 0 10 0\n\
   3: 0100007F:A8C2 0100007F:1F91 01 00000000:00000000 00:00000000 00000000  1000        0 55556 1 0000000000000000 100 0 0 10 0\n\
   4: 0100007F:1F91 0100007F:A8C3 01 00000000:00000000 00:00000000 00000000  1000        0 0 1 0000000000000000 100 0 0 10 0\n\
   5: 0100007F:1F91 0100007F:A8C4 06 00000000:00000000 00:00000000 00000000  1000        0 0 1 0000000000000000 100 0 0 10 0\n";

    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0000000000000000FFFF00000100007F:1F91 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 77777 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000000000000000000001000000:1F92 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 77778 1 0000000000000000 100 0 0 10 0\n\
   2: 00000000000000000000000000000000:0050 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 77779 1 0000000000000000 100 0 0 10 0\n";

    /// One process holds two loopback listeners: a debug agent on 5005 and
    /// the server on 8081, with the debug row first.
    const TWO_LISTENERS: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:138D 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 66661 1 0000000000000000 100 0 0 10 0\n\
   1: 0100007F:1F91 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 66662 1 0000000000000000 100 0 0 10 0\n";

    fn address(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn only_loopback_rows_in_listen_or_established_state_are_read() {
        let sockets = parse_proc_net_tcp(TCP);

        assert_eq!(
            sockets.iter().map(|s| s.inode).collect::<Vec<_>>(),
            [55555, 55557, 55556, 0]
        );
        assert!(sockets[0].listening);
        assert_eq!(sockets[0].local, address("127.0.0.1:8081"));
        assert!(!sockets[1].listening);
        assert_eq!(sockets[1].remote, address("127.0.0.1:43202"));
    }

    #[test]
    fn a_mapped_ipv4_listener_answers_as_ipv4_and_ipv6_loopback_as_itself() {
        let sockets = parse_proc_net_tcp(TCP6);

        assert_eq!(
            sockets
                .iter()
                .map(|socket| (socket.local, socket.inode))
                .collect::<Vec<_>>(),
            [
                (address("127.0.0.1:8081"), 77777),
                (address("[::1]:8082"), 77778),
            ]
        );
    }

    #[test]
    fn the_listener_a_unit_process_holds_is_the_owned_one() {
        let sockets = parse_proc_net_tcp(TCP);
        let held: HashSet<u64> = [55555, 55556].into_iter().collect();
        let other: HashSet<u64> = [10811].into_iter().collect();
        // An established socket is never a listener, whoever holds it.
        let established: HashSet<u64> = [55557].into_iter().collect();

        assert_eq!(
            owned_by(&sockets, &held, 8081),
            Some(address("127.0.0.1:8081"))
        );
        assert_eq!(owned_by(&sockets, &other, 22), None);
        assert_eq!(owned_by(&sockets, &established, 8081), None);
    }

    /// A unit that holds no loopback listener is still starting, and one
    /// that holds another port is not one this plugin started.
    #[test]
    fn the_ports_one_process_listens_on_separate_starting_from_foreign() {
        let sockets = parse_proc_net_tcp(TWO_LISTENERS);
        let held: HashSet<u64> = [66661, 66662].into_iter().collect();
        let none: HashSet<u64> = [12345].into_iter().collect();

        assert_eq!(held_ports(&sockets, &held), [5005, 8081]);
        assert!(held_ports(&sockets, &none).is_empty());
    }

    /// A JVM with a debug agent holds two loopback listeners, and the debug
    /// one may come first. The unit's own `--port` picks the server.
    #[test]
    fn the_port_of_the_command_line_picks_the_listener() {
        let sockets = parse_proc_net_tcp(TWO_LISTENERS);
        let held: HashSet<u64> = [66661, 66662].into_iter().collect();
        let port = port_of(
            "/usr/lib/jvm/default/bin/java -cp /x/languagetool-server.jar \
org.languagetool.server.HTTPServer --port 8081 --config /run/x.properties",
        )
        .expect("the command line names a port");

        assert_eq!(port, 8081);
        assert_eq!(
            owned_by(&sockets, &held, port),
            Some(address("127.0.0.1:8081"))
        );
        // The debug listener is the first row, and it is never the answer.
        assert_eq!(
            owned_by(&sockets, &held, 5005),
            Some(address("127.0.0.1:5005"))
        );
    }

    #[test]
    fn the_port_is_read_from_the_command_line() {
        assert_eq!(
            port_of("/usr/bin/x --http --port 43210 --config /a"),
            Some(43210)
        );
        assert_eq!(port_of("/usr/bin/x --port"), None);
        assert_eq!(port_of("/usr/bin/x --port many"), None);
        assert_eq!(port_of("/usr/bin/x"), None);
    }

    /// The server end of a connection is the row whose local address is the
    /// server's and whose remote address is the client's. A row with inode 0
    /// is a connection that still waits in the accept queue.
    #[test]
    fn the_server_end_of_a_connection_is_read_by_both_addresses() {
        let sockets = parse_proc_net_tcp(TCP);
        let server = address("127.0.0.1:8081");

        assert_eq!(
            accepted_inode(&sockets, server, address("127.0.0.1:43202")),
            Some(55557)
        );
        assert_eq!(
            accepted_inode(&sockets, server, address("127.0.0.1:43203")),
            None,
            "still in the accept queue"
        );
        assert_eq!(
            accepted_inode(&sockets, server, address("127.0.0.1:43204")),
            None,
            "a closing connection is not an accepted one"
        );
        assert_eq!(
            accepted_inode(&sockets, address("127.0.0.1:43202"), server),
            Some(55556),
            "the addresses swapped name the client end, a different socket"
        );
    }

    #[test]
    fn the_show_output_is_read_by_key() {
        let facts = parse_show(
            "ActiveState=active\nMainPID=4242\nTransient=yes\nExecStart={ path=/usr/lib/jvm/default/bin/java ; argv[]=/usr/lib/jvm/default/bin/java -cp /home/x/a b/x.jar org.languagetool.server.HTTPServer --port 8081 ; ignore_errors=no ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }\n",
        );

        assert_eq!(facts.active_state, "active");
        assert_eq!(facts.main_pid, 4242);
        assert!(facts.transient);
        assert_eq!(facts.exec_start.path, "/usr/lib/jvm/default/bin/java");
        assert_eq!(
            facts.exec_start.argv,
            "/usr/lib/jvm/default/bin/java -cp /home/x/a b/x.jar org.languagetool.server.HTTPServer --port 8081"
        );
        assert!(is_active(&facts.active_state));
        assert!(!is_active("inactive"));

        let file_unit = parse_show("ActiveState=active\nMainPID=1\nTransient=no\nExecStart=\n");
        assert!(!file_unit.transient);
        assert_eq!(file_unit.exec_start, ExecStart::default());
    }

    #[test]
    fn socket_links_are_read() {
        assert_eq!(socket_inode("socket:[55555]"), Some(55555));
        assert_eq!(socket_inode("/dev/null"), None);
    }

    /// The proof runs against a listener this process holds: the accepted
    /// end is ours, so our pid passes and any other pid is refused.
    #[test]
    fn a_connection_is_proven_against_the_process_that_accepted_it() {
        let server = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = server.local_addr().unwrap();
        let (release, released) = mpsc::channel::<()>();
        let accepting = std::thread::spawn(move || {
            let (accepted, _) = server.accept().expect("the client connects");
            // Hold the server end open until the proofs below are done.
            let _ = released.recv();
            drop(accepted);
        });

        let stream = TcpStream::connect(address).expect("the listener answers");

        accepted_by(&stream, std::process::id()).expect("this process accepted it");
        let refused = accepted_by(&stream, 1).expect_err("init did not accept it");
        assert!(refused.contains("other than 1"), "{refused}");

        let _ = release.send(());
        accepting.join().unwrap();
    }
}
