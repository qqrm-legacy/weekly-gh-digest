//! Loopback-only HTTP fixtures and deterministic time; never use provider credentials.
use crate::clock::Clock;
use chrono::{DateTime, Utc};
use std::{
    cell::{Cell, RefCell},
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

pub struct FakeClock {
    pub now: Cell<DateTime<Utc>>,
    pub sleeps: RefCell<Vec<Duration>>,
}

impl FakeClock {
    pub fn at(timestamp: &str) -> Self {
        Self {
            now: Cell::new(timestamp.parse().unwrap()),
            sleeps: RefCell::new(vec![]),
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now.get()
    }
    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
        self.now
            .set(self.now.get() + chrono::Duration::from_std(duration).unwrap());
    }
}

pub struct TestServer {
    pub url: String,
    worker: thread::JoinHandle<Vec<String>>,
}

impl TestServer {
    pub fn start(responses: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let mut requests = vec![];
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "expected fixture request did not arrive"
                            );
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("fixture listener failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    assert!(read > 0, "request closed before its body");
                    bytes.extend_from_slice(&buffer[..read]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        Self { url, worker }
    }

    pub fn finish(self) -> Vec<String> {
        self.worker.join().unwrap()
    }
}
