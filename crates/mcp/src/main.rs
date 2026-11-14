use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use notify_mcp::api::{DEFAULT_TIMEOUT, NotifyApi};

const USAGE: &str = "\
nsh-notify-mcp — the notification inbox of nsh-mail, for agents

Usage:
  nsh-notify-mcp --server URL --key nb_…                stdio (what MCP clients launch)
  nsh-notify-mcp --http 127.0.0.1:8099 --server URL      streamable HTTP at POST /mcp

Options:
  --server URL    base URL of the nsh-mail server (env NSH_NOTIFY_SERVER)
  --key KEY       API key starting with nb_ (env NSH_NOTIFY_KEY); required for
                  stdio, optional for --http, where each request may bring its own
  --http ADDR     listen for MCP over HTTP instead of stdio
  --timeout SECS  give up on a REST call after this many seconds (default 15)
  -h, --help      this text
  -V, --version   the version

Environment:
  NSH_NOTIFY_SERVER, NSH_NOTIFY_KEY   the same as --server and --key

Examples:
  nsh-notify-mcp --server https://mail-api.bangwu.me --key nb_xxxxxxxx
  nsh-notify-mcp --http 127.0.0.1:8099 --server http://127.0.0.1:8080 --key nb_xxxxxxxx
";

struct Options {
    server: String,
    key: Option<String>,
    http: Option<SocketAddr>,
    timeout: Duration,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("nsh-notify-mcp: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match run(&options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("nsh-notify-mcp: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(options: &Options) -> Result<(), String> {
    match options.http {
        Some(addr) => {
            let router = notify_mcp::client_router(
                options.server.clone(),
                options.key.clone(),
                options.timeout,
            );
            let listener = notify_mcp::http::bind(addr).await.map_err(|e| {
                format!("cannot listen on {addr}: {e} (is another process using that port?)")
            })?;
            let auth = match &options.key {
                Some(_) => "--key (requests must send it)",
                None => "each request's own Bearer key",
            };
            eprintln!(
                "nsh-notify-mcp {}: {addr} over HTTP, inbox {}, auth: {auth}",
                env!("CARGO_PKG_VERSION"),
                options.server
            );
            notify_mcp::http::serve(listener, router)
                .await
                .map_err(|e| format!("HTTP transport stopped: {e}"))
        }
        None => {
            let key = options.key.clone().ok_or(
                "stdio mode needs an API key: pass --key nb_… or set NSH_NOTIFY_KEY \
                 (an MCP client's config can provide either)",
            )?;
            let backend = NotifyApi::new(options.server.clone(), key).with_timeout(options.timeout);
            eprintln!(
                "nsh-notify-mcp {}: stdio, inbox {}",
                env!("CARGO_PKG_VERSION"),
                options.server
            );
            notify_mcp::stdio::serve(Arc::new(backend))
                .await
                .map_err(|e| format!("stdio transport stopped: {e}"))
        }
    }
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut server = std::env::var("NSH_NOTIFY_SERVER").ok();
    let mut key = std::env::var("NSH_NOTIFY_KEY").ok();
    let mut http = None;
    let mut timeout = DEFAULT_TIMEOUT;
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].as_str();
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (arg, None),
        };
        let value = |index: &mut usize| -> Result<String, String> {
            if let Some(value) = &inline {
                return Ok(value.clone());
            }
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match name {
            "-h" | "--help" => return Ok(None),
            "-V" | "--version" => {
                println!("nsh-notify-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--server" => server = Some(value(&mut index)?),
            "--key" => key = Some(value(&mut index)?),
            "--http" => {
                let addr = value(&mut index)?;
                http =
                    Some(addr.parse::<SocketAddr>().map_err(|_| {
                        format!("--http {addr}: use HOST:PORT, e.g. 127.0.0.1:8099")
                    })?);
            }
            "--timeout" => {
                let secs = value(&mut index)?;
                let secs: f64 = secs
                    .parse()
                    .map_err(|_| format!("--timeout {secs}: give a number of seconds"))?;
                if !(1.0..=300.0).contains(&secs) {
                    return Err(format!(
                        "--timeout {secs}: keep it between 1 and 300 seconds"
                    ));
                }
                timeout = Duration::from_secs_f64(secs);
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => return Err(format!("unexpected argument {other}")),
        }
        index += 1;
    }

    let server = server
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("no inbox server: pass --server https://… or set NSH_NOTIFY_SERVER")?;
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err(format!(
            "--server {server}: start it with http:// or https://"
        ));
    }
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    if let Some(key) = &key
        && !key.starts_with("nb_")
    {
        return Err(
            "--key: the API key starts with nb_ (create one in Settings → API keys)".to_string(),
        );
    }
    Ok(Some(Options {
        server,
        key,
        http,
        timeout,
    }))
}
