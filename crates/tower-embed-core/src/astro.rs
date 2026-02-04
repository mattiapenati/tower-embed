//! Embedding support for Astro projects.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use crate::{Body, BoxError, response};

/// Builds the Astro project and the return the folder containing generated files.
pub fn build_project(root: &Path) -> std::io::Result<PathBuf> {
    let pm = PackageManager::from_project_folder(root)
        .ok_or_else(|| std::io::Error::other("Failed to detect package manager"))?;
    pm.build(root)
}

/// Package managers supported by Astro projects.
#[derive(Clone, Copy)]
enum PackageManager {
    Npm,
    Yarn,
    Pnpm,
    Deno,
}

impl PackageManager {
    /// Detects the package manager used in the given project folder using the existing lock file.
    fn from_project_folder(project: &Path) -> Option<Self> {
        const DENO_LOCK: &str = "deno.lock";
        const YARN_LOCK: &str = "yarn.lock";
        const PNPM_LOCK: &str = "pnpm-lock.yaml";
        const NPM_LOCK: &str = "package-lock.json";

        if project.join(DENO_LOCK).exists() {
            Some(PackageManager::Deno)
        } else if project.join(YARN_LOCK).exists() {
            Some(PackageManager::Yarn)
        } else if project.join(PNPM_LOCK).exists() {
            Some(PackageManager::Pnpm)
        } else if project.join(NPM_LOCK).exists() {
            Some(PackageManager::Npm)
        } else {
            None
        }
    }

    fn command(self) -> std::process::Command {
        use std::process::Command;

        let mut cmd = match self {
            PackageManager::Npm => Command::new("npm"),
            PackageManager::Yarn => Command::new("yarn"),
            PackageManager::Pnpm => Command::new("pnpm"),
            PackageManager::Deno => Command::new("deno"),
        };

        match self {
            PackageManager::Deno => cmd.arg("task"),
            _ => cmd.arg("run"),
        };

        cmd
    }

    /// Run the development server and returns the listening port.
    pub fn dev(self, project: &Path) -> std::io::Result<std::net::SocketAddr> {
        use std::{
            io::{BufRead, BufReader, LineWriter, Write},
            process::Stdio,
        };

        let mut command = self.command();
        command
            .arg("dev")
            .current_dir(project)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if stdout_has_colors() {
            command.env("FORCE_COLOR", "1");
        }
        let mut child = command.spawn()?;

        let mut stdout = LineWriter::new(std::io::stdout());
        let mut child_stdout = child.stdout.as_mut().map(BufReader::new).unwrap();

        let mut address = None;
        for line in (&mut child_stdout).lines() {
            let line = line?;
            writeln!(&mut stdout, "{line}")?;
            stdout.flush()?;

            const PATTERN: &str = "http://";
            if let Some(index) = line.trim().find(PATTERN) {
                address = Some(
                    line[index + PATTERN.len()..]
                        .chars()
                        // Listening URL has a trailing slash
                        .take_while(|c| *c != '/')
                        .collect::<String>()
                        .parse::<std::net::SocketAddr>()
                        .unwrap(),
                );
                break;
            }
        }
        stdout.write_all(child_stdout.buffer())?;

        let address = match address {
            Some(address) => address,
            None => {
                child.kill()?;
                return Err(std::io::Error::other(
                    "Failed to find Astro dev server port",
                ));
            }
        };

        // Forward the remaining stdout
        std::thread::spawn(move || {
            if let Some(child_stdout) = child.stdout {
                let mut child_stdout = BufReader::new(child_stdout);
                let _ = std::io::copy(&mut child_stdout, &mut stdout).ok();
            }
        });

        Ok(address)
    }

    /// Builds the Astro project and the return the folder containing generated files.
    fn build(self, project: &Path) -> std::io::Result<PathBuf> {
        let exit_status = self
            .command()
            .arg("build")
            .current_dir(project)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?
            .wait()?;

        if !exit_status.success() {
            return Err(std::io::Error::other("Astro build failed"));
        }

        Ok(project.join("dist"))
    }
}

/// Checks if stdout supports colors.
fn stdout_has_colors() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// A proxy to the Astro dev server.
pub struct AstroProxy {
    pool: bb8::Pool<PoolManager>,
}

impl AstroProxy {
    /// Creates a new `AstroProxy` by starting the Astro dev server in the given project root.
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let pm = PackageManager::from_project_folder(root)
            .ok_or_else(|| std::io::Error::other("Failed to detect package manager"))?;
        let port = pm.dev(root)?;

        let pool = bb8::Pool::builder().build_unchecked(PoolManager { port: port.port() });
        Ok(Self { pool })
    }

    /// Sends an HTTP request to the Astro dev server and returns the response.
    pub async fn send_request(&self, req: http::Request<()>) -> http::Response<Body> {
        let mut conn = match self.get_connection().await {
            Ok(conn) => conn,
            Err(_) => return response::internal_server_error(),
        };
        match conn.send_request(req).await {
            Ok(res) => res,
            Err(_) => response::internal_server_error(),
        }
    }

    async fn get_connection(&self) -> Result<bb8::PooledConnection<'_, PoolManager>, BoxError> {
        let conn = self.pool.get().await?;
        Ok(conn)
    }
}

struct PoolManager {
    port: u16,
}

impl bb8::ManageConnection for PoolManager {
    type Connection = Connection;
    type Error = ConnectionError;

    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        Connection::new(self.port)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        conn.0.ready().await.map_err(ConnectionError::Hyper)
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.0.is_closed()
    }
}

struct Connection(hyper::client::conn::http1::SendRequest<Body>);

#[derive(Debug)]
enum ConnectionError {
    Io(std::io::Error),
    Hyper(hyper::Error),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Io(err) => std::fmt::Display::fmt(err, f),
            ConnectionError::Hyper(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl std::error::Error for ConnectionError {}

impl Connection {
    async fn new(port: u16) -> Result<Self, ConnectionError> {
        let socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(ConnectionError::Io)?;
        let io = hyper_util::rt::TokioIo::new(socket);
        let (send, connection) = hyper::client::conn::http1::handshake::<_, Body>(io)
            .await
            .map_err(ConnectionError::Hyper)?;

        tokio::spawn(async move {
            let _ = connection.with_upgrades().await;
        });

        Ok(Self(send))
    }

    async fn send_request(
        &mut self,
        req: http::Request<()>,
    ) -> Result<http::Response<Body>, BoxError> {
        let req = req.map(|_| Body::empty());

        if req.method() == http::Method::GET && req.headers().contains_key(http::header::UPGRADE) {
            // Duplicate the request for server and client sides
            let (parts, body) = req.into_parts();
            let server_req = http::Request::from_parts(parts.clone(), Body::empty());
            let client_req = http::Request::from_parts(parts, body);

            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let mut response = self.0.send_request(client_req).await.unwrap();

            tokio::spawn(async move {
                let client = hyper::upgrade::on(&mut response).await.unwrap();
                let mut client = hyper_util::rt::TokioIo::new(client);

                response_tx.send(response).unwrap();

                let server = hyper::upgrade::on(server_req).await.unwrap();
                let mut server = hyper_util::rt::TokioIo::new(server);

                tokio::io::copy_bidirectional(&mut client, &mut server)
                    .await
                    .unwrap()
            });

            let response = response_rx.await.unwrap();
            return Ok(response.map(Body::new));
        }

        let response = self.0.send_request(req).await.unwrap();
        Ok(response.map(Body::new))
    }
}
