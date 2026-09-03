use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

pub(super) struct TestServer {
    pub(super) base: String,
    listener: TcpListener,
}

impl TestServer {
    pub(super) fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        Self { base, listener }
    }

    pub(super) fn serve(self, routes: HashMap<String, Vec<u8>>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            for _ in 0..routes.len() {
                let (mut stream, _) = self.listener.accept().unwrap();
                let mut request = [0_u8; 8192];
                let read = stream.read(&mut request).unwrap();
                let first_line = std::str::from_utf8(&request[..read])
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap();
                let path = first_line.split_whitespace().nth(1).unwrap();
                let body = routes
                    .get(path)
                    .unwrap_or_else(|| panic!("unexpected path {path}"));
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        })
    }
}
