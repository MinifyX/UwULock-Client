//! A tiny HTTP server for fakes: every request goes to one handler, which
//! answers with a status, a content type and a body. Plain HTTP on
//! 127.0.0.1, port chosen by the system, one thread per request. Every request
//! is kept, so a test can look at what the client sent.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// Without the query.
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub struct Answer {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn json(status: u16, body: serde_json::Value) -> Answer {
        Answer {
            status,
            content_type: "application/json",
            body: body.to_string().into_bytes(),
        }
    }

    pub fn bytes(content_type: &'static str, body: Vec<u8>) -> Answer {
        Answer {
            status: 200,
            content_type,
            body,
        }
    }

    pub fn empty(status: u16) -> Answer {
        Answer {
            status,
            content_type: "text/plain",
            body: Vec::new(),
        }
    }
}

type Handler = dyn Fn(&Request) -> Answer + Send + Sync;

pub struct FakeHttp {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
}

impl FakeHttp {
    pub fn start(handler: impl Fn(&Request) -> Answer + Send + Sync + 'static) -> FakeHttp {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handler: Arc<Handler> = Arc::new(handler);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let handler = handler.clone();
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let _ = serve(stream, &*handler, &seen);
                });
            }
        });
        FakeHttp { url, requests }
    }

    /// `METHOD /path` of every request so far.
    pub fn calls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect()
    }

    pub fn last(&self, method: &str, path: &str) -> Option<Request> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|r| r.method == method && r.path == path)
            .cloned()
    }
}

fn serve(
    mut stream: TcpStream,
    handler: &Handler,
    seen: &Mutex<Vec<Request>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (target, String::new()),
    };
    let request = Request {
        method,
        path,
        query,
        headers,
        body,
    };
    seen.lock().unwrap().push(request.clone());
    let answer = handler(&request);
    write!(
        stream,
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        answer.status,
        answer.content_type,
        answer.body.len()
    )?;
    stream.write_all(&answer.body)
}
