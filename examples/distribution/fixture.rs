//! Loopback fixtures own their server and temporary files until checks finish.
use super::support::{self, MAX_BYTES, OwnedChild, Result, require};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Https {
    pub base: String,
    pub certificate: PathBuf,
    directory: PathBuf,
    _child: OwnedChild,
}
impl Https {
    pub fn start(root: &Path) -> Result<Self> {
        let directory = root.join("http-responses");
        fs::create_dir(&directory)?;
        let certificate = root.join("localhost.pem");
        let key = root.join("localhost.key");
        support::run(
            support::command("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "1",
                    "-subj",
                    "/CN=localhost",
                    "-addext",
                    "subjectAltName=DNS:localhost",
                    "-keyout",
                ])
                .arg(&key)
                .arg("-out")
                .arg(&certificate),
        )?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        drop(listener);
        // OpenSSL's -HTTP mode serves complete response files, including deliberately
        // truncated bodies. Certificates and responses belong only to this fixture.
        let mut child = OwnedChild(
            support::command("openssl")
                .args([
                    "s_server",
                    "-quiet",
                    "-HTTP",
                    "-http_server_binmode",
                    "-accept",
                    &address.to_string(),
                    "-cert",
                ])
                .arg(&certificate)
                .arg("-key")
                .arg(key)
                .current_dir(&directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            require(
                child.0.try_wait()?.is_none(),
                "HTTPS fixture exited during startup",
            )?;
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                break;
            }
            require(Instant::now() < deadline, "HTTPS fixture did not start")?;
            thread::sleep(Duration::from_millis(25));
        }
        Ok(Self {
            base: format!("https://localhost:{}", address.port()),
            certificate,
            directory,
            _child: child,
        })
    }
    pub fn serve(&self, name: &str, data: &[u8], missing_bytes: usize) -> Result<()> {
        self.response(name, "200 OK", data, missing_bytes)
    }
    pub fn missing(&self, name: &str) -> Result<()> {
        self.response(name, "404 Not Found", b"missing", 0)
    }
    fn response(&self, name: &str, status: &str, data: &[u8], missing_bytes: usize) -> Result<()> {
        require(
            !name.contains(['/', '\\']) && data.len() as u64 <= MAX_BYTES,
            "invalid fixture response",
        )?;
        let mut file = fs::File::create(self.directory.join(name))?;
        write!(
            file,
            "HTTP/1.0 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            data.len() + missing_bytes
        )?;
        file.write_all(data)?;
        Ok(())
    }
}

pub struct Http {
    pub base: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}
impl Http {
    pub fn start(directory: &Path) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let base = format!("http://{}", listener.local_addr()?);
        let directory = directory.to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets inherit nonblocking mode on some hosts.
                        stream.set_nonblocking(false)?;
                        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
                        // Disconnects are normal when a package manager probes a URL.
                        if let Err(error) = respond(&mut stream, &directory) {
                            eprintln!("package fixture request failed: {error}");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        });
        Ok(Self {
            base,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn respond(stream: &mut TcpStream, directory: &Path) -> Result<()> {
    let mut request = Vec::new();
    let mut reader = std::io::Read::take(BufReader::new(&mut *stream), 16385);
    reader.read_until(b'\n', &mut request)?;
    require(request.len() <= 4096, "HTTP fixture request is too long")?;
    let mut total = request.len();
    loop {
        let mut header = Vec::new();
        let length = reader.read_until(b'\n', &mut header)?;
        total += length;
        require(
            length > 0 && total <= 16384,
            "HTTP fixture headers are incomplete or too long",
        )?;
        if header == b"\r\n" || header == b"\n" {
            break;
        }
    }
    let request = std::str::from_utf8(&request)?;
    let mut parts = request.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?;
    let name = parts
        .next()
        .and_then(|p| p.strip_prefix('/'))
        .ok_or("missing HTTP path")?;
    require(
        matches!(method, "GET" | "HEAD")
            && !name.is_empty()
            && !name.contains(['/', '\\', '%'])
            && name != "..",
        "invalid fixture request",
    )?;
    let (status, data) = match support::read(&directory.join(name), MAX_BYTES) {
        Ok(data) => ("200 OK", data),
        Err(_) => ("404 Not Found", b"missing".to_vec()),
    };
    write!(
        stream,
        "HTTP/1.0 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        data.len()
    )?;
    if method == "GET" {
        stream.write_all(&data)?;
    }
    Ok(())
}
