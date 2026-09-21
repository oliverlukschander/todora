#[cfg(any(
    feature = "multiplayer-test",
    all(target_os = "macos", feature = "game-center")
))]
use super::session::MAX_PACKET;
use super::session::Packet;
use bevy::prelude::*;

// Some event kinds exist only when their platform transport is compiled.
#[cfg_attr(
    not(all(target_os = "macos", feature = "game-center")),
    allow(dead_code)
)]
pub enum Event {
    Status(String),
    Connected(bool, String),
    Data(Vec<u8>),
    End(String),
}

#[derive(Resource, Default)]
pub struct Transport {
    pending: Vec<Event>,
    #[cfg(feature = "multiplayer-test")]
    local: Option<local::Link>,
}

impl Transport {
    pub fn start(&mut self) {
        self.leave();
        self.pending.clear();
        #[cfg(feature = "multiplayer-test")]
        if let Ok(role) = std::env::var("TODORA_LOCAL_PEER") {
            match local::Link::new(&role) {
                Ok(link) => self.local = Some(link),
                Err(error) => self.pending.push(Event::End(error)),
            }
            return;
        }
        #[cfg(all(target_os = "macos", feature = "game-center"))]
        unsafe {
            // Ignore queued events from a previous, already-left match.
            let mut bytes = [0u8; MAX_PACKET];
            let mut len = 0;
            while native::todora_gc_poll(bytes.as_mut_ptr(), bytes.len(), &mut len) != 0 {}
            native::todora_gc_start();
        }
        #[cfg(not(all(target_os = "macos", feature = "game-center")))]
        self.pending.push(Event::End(
            "This build does not include Game Center.".into(),
        ));
    }

    pub fn leave(&mut self) {
        #[cfg(feature = "multiplayer-test")]
        {
            self.local = None;
        }
        #[cfg(all(target_os = "macos", feature = "game-center"))]
        unsafe {
            native::todora_gc_leave();
        }
    }

    pub fn send(&mut self, packet: &Packet) {
        let bytes = packet.encode();
        #[cfg(feature = "multiplayer-test")]
        if let Some(local) = &mut self.local {
            local.send(&bytes);
            return;
        }
        #[cfg(all(target_os = "macos", feature = "game-center"))]
        unsafe {
            native::todora_gc_send(bytes.as_ptr(), bytes.len(), packet.reliable());
        }
        #[cfg(not(all(target_os = "macos", feature = "game-center")))]
        let _ = (bytes, packet.reliable());
    }

    pub fn poll(&mut self) -> Vec<Event> {
        let events = std::mem::take(&mut self.pending);
        #[cfg(any(
            feature = "multiplayer-test",
            all(target_os = "macos", feature = "game-center")
        ))]
        let mut events = events;
        #[cfg(feature = "multiplayer-test")]
        if let Some(local) = &mut self.local {
            events.extend(local.poll());
            return events;
        }
        #[cfg(all(target_os = "macos", feature = "game-center"))]
        for _ in 0..256 {
            let mut bytes = [0u8; MAX_PACKET];
            let mut len = 0;
            // The bridge copies at most capacity bytes and never retains the pointer.
            let kind = unsafe { native::todora_gc_poll(bytes.as_mut_ptr(), bytes.len(), &mut len) };
            if kind == 0 {
                break;
            }
            if len > bytes.len() {
                break;
            }
            let data = &bytes[..len];
            let text = || String::from_utf8_lossy(data).into_owned();
            match kind {
                1 => events.push(Event::Status(text())),
                2 if !data.is_empty() => events.push(Event::Connected(
                    data[0] == 1,
                    String::from_utf8_lossy(&data[1..]).into_owned(),
                )),
                3 => events.push(Event::Data(data.to_vec())),
                4 => events.push(Event::End(text())),
                _ => {}
            }
        }
        events
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.leave();
    }
}

#[cfg(all(target_os = "macos", feature = "game-center"))]
mod native {
    unsafe extern "C" {
        pub fn todora_gc_start();
        pub fn todora_gc_leave();
        pub fn todora_gc_send(bytes: *const u8, length: usize, reliable: bool);
        pub fn todora_gc_poll(bytes: *mut u8, capacity: usize, length: *mut usize) -> i32;
        pub fn todora_gc_support_directory(bytes: *mut u8, capacity: usize) -> usize;
    }
}

#[cfg(all(target_os = "macos", feature = "game-center"))]
pub(crate) fn support_directory() -> Option<std::path::PathBuf> {
    let mut bytes = [0u8; 4096];
    // Foundation resolves the container's directory when the app is sandboxed.
    let len = unsafe { native::todora_gc_support_directory(bytes.as_mut_ptr(), bytes.len()) };
    if len == 0 || len > bytes.len() {
        return None;
    }
    Some(std::path::PathBuf::from(
        std::str::from_utf8(&bytes[..len]).ok()?,
    ))
}

// Development-only transport for two processes on one Mac. It cannot listen
// on the LAN and is excluded from the Game Center and normal release builds.
#[cfg(feature = "multiplayer-test")]
mod local {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    pub struct Link {
        host: bool,
        listener: Option<TcpListener>,
        stream: Option<TcpStream>,
        input: Vec<u8>,
        output: Vec<u8>,
        attempt: Instant,
        failed: bool,
    }
    impl Link {
        pub fn new(role: &str) -> Result<Self, String> {
            let host = match role {
                "host" => true,
                "join" => false,
                _ => return Err("TODORA_LOCAL_PEER must be host or join".into()),
            };
            let listener = if host {
                let l =
                    TcpListener::bind((Ipv4Addr::LOCALHOST, 47653)).map_err(|e| e.to_string())?;
                l.set_nonblocking(true).map_err(|e| e.to_string())?;
                Some(l)
            } else {
                None
            };
            Ok(Self {
                host,
                listener,
                stream: None,
                input: vec![],
                output: vec![],
                attempt: Instant::now() - Duration::from_secs(1),
                failed: false,
            })
        }
        pub fn send(&mut self, bytes: &[u8]) {
            if bytes.len() > MAX_PACKET || self.output.len() > 64 * 1024 {
                self.failed = true;
                return;
            }
            self.output
                .extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            self.output.extend_from_slice(bytes);
        }
        pub fn poll(&mut self) -> Vec<Event> {
            if self.failed {
                return vec![Event::End("Local test connection closed.".into())];
            }
            let mut events = vec![];
            if self.stream.is_none() && self.attempt.elapsed() >= Duration::from_millis(200) {
                self.attempt = Instant::now();
                let result = if let Some(listener) = &self.listener {
                    listener.accept().map(|x| x.0)
                } else {
                    TcpStream::connect_timeout(
                        &SocketAddr::from((Ipv4Addr::LOCALHOST, 47653)),
                        Duration::from_millis(10),
                    )
                };
                if let Ok(stream) = result {
                    if stream.set_nonblocking(true).is_err() || stream.set_nodelay(true).is_err() {
                        self.failed = true;
                        return vec![Event::End("Could not configure local test socket.".into())];
                    }
                    self.stream = Some(stream);
                    events.push(Event::Connected(
                        self.host,
                        if self.host {
                            "Local driver B"
                        } else {
                            "Local driver A"
                        }
                        .into(),
                    ));
                }
            }
            if let Some(stream) = &mut self.stream {
                if !self.output.is_empty() {
                    match stream.write(&self.output) {
                        Ok(0) => self.failed = true,
                        Ok(n) => {
                            self.output.drain(..n);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => self.failed = true,
                    }
                }
                let mut bytes = [0u8; 8192];
                for _ in 0..8 {
                    match stream.read(&mut bytes) {
                        Ok(0) => {
                            self.failed = true;
                            break;
                        }
                        Ok(n) => self.input.extend_from_slice(&bytes[..n]),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            self.failed = true;
                            break;
                        }
                    }
                }
                while self.input.len() >= 2 {
                    let len = u16::from_le_bytes([self.input[0], self.input[1]]) as usize;
                    if len > MAX_PACKET {
                        self.failed = true;
                        break;
                    }
                    if self.input.len() < len + 2 {
                        break;
                    }
                    events.push(Event::Data(self.input[2..len + 2].to_vec()));
                    self.input.drain(..len + 2);
                }
            }
            events
        }
    }
}
