//! reclass-server: owns the attached process and the project, and serves the
//! web UI. Clients drive it with the same commands over a WebSocket (`/ws`) or
//! plain HTTP (`POST /api/rpc`); every change is pushed to all WebSocket
//! clients, so a CLI and the browser stay in sync.

mod access;
mod canvas;
mod workspace;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        Mutex,
    },
    time::Duration,
};

use axum::{
    extract::{
        ws::{
            Message,
            WebSocket,
        },
        State,
        WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use clap::Parser;
use futures_util::{
    SinkExt,
    StreamExt,
};
use reclass_core::{
    demo::{
        demo_project,
        DemoProvider,
        DEMO_PID,
    },
    driver::DriverProvider,
    project::ProjectFile,
    source::ProcessProvider,
};
use serde_json::{
    json,
    Value,
};
use tokio::sync::{
    broadcast,
    mpsc,
};
use tower_http::services::{
    ServeDir,
    ServeFile,
};
use workspace::Workspace;

#[derive(Parser)]
#[command(about = "reclass-rs server: memory exploration over HTTP/WebSocket")]
struct Args {
    /// Use the built-in simulated process instead of the kernel driver.
    #[arg(long)]
    demo: bool,
    /// Address to listen on. Anything but loopback exposes process memory to the network.
    #[arg(long, default_value = "127.0.0.1:7878")]
    bind: SocketAddr,
    /// Project file to load on start and save to.
    #[arg(long)]
    project: Option<PathBuf>,
    /// Directory with the built web UI.
    #[arg(long, default_value = "web/app/dist")]
    static_dir: PathBuf,
    /// Module holding the XenuineDecrypt indirection (defaults to the process image).
    #[arg(long)]
    decrypt_module: Option<String>,
    /// Attach to this process id on start.
    #[arg(long)]
    pid: Option<u32>,
    /// Live update interval in milliseconds.
    #[arg(long, default_value_t = 250)]
    tick_ms: u64,
    /// Additional browser origin to trust, e.g. http://localhost:5173. Repeatable.
    #[arg(long, value_parser = access::parse_origin)]
    allowed_origin: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    ws: Arc<Mutex<Workspace>>,
    events: broadcast::Sender<Arc<str>>,
}

impl AppState {
    fn publish(&self, v: &Value) {
        // No receivers is fine.
        let _ = self.events.send(Arc::from(v.to_string()));
    }

    /// Runs a command, broadcasts what changed and returns the reply body.
    async fn exec(&self, method: String, params: Value) -> Value {
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut ws = state.ws.lock().unwrap();
            match ws.handle(&method, &params) {
                Ok((result, changed)) => {
                    if changed.session {
                        state.publish(&ws.session());
                    }
                    if changed.defs {
                        state.publish(&ws.defs());
                    }
                    let frame = ws.frame();
                    state.publish(&frame);
                    json!({ "ok": true, "result": result })
                }
                Err(error) => json!({ "ok": false, "error": error }),
            }
        })
        .await
        .unwrap_or_else(|e| json!({ "ok": false, "error": format!("command panicked: {e}") }))
    }
}

async fn rpc(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let params = body.get("params").cloned().unwrap_or(Value::Null);
    Json(state.exec(method, params).await)
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.protocols(["reclass"])
        .on_upgrade(move |socket| client(socket, state))
}

fn api_router(state: AppState, access: access::Access) -> Router {
    Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/api/rpc", post(rpc))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::new(access),
            access::authorize,
        ))
        .with_state(state)
}

async fn client(socket: WebSocket, state: AppState) {
    let (mut sink, mut stream) = socket.split();
    let mut events = state.events.subscribe();
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<String>();

    let hello = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            let mut ws = state.ws.lock().unwrap();
            [ws.session(), ws.defs(), ws.frame()].map(|v| v.to_string())
        })
        .await
        .unwrap()
    };
    for msg in hello {
        if sink.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    let writer = tokio::spawn(async move {
        loop {
            let msg: String = tokio::select! {
                ev = events.recv() => match ev {
                    Ok(m) => m.to_string(),
                    // A slow client just skips stale frames.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                },
                r = reply_rx.recv() => match r { Some(m) => m, None => break },
            };
            if sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(req) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let state = state.clone();
        let reply_tx = reply_tx.clone();
        tokio::spawn(async move {
            let mut reply = state.exec(method, params).await;
            reply["type"] = json!("reply");
            reply["id"] = id;
            let _ = reply_tx.send(reply.to_string());
        });
    }
    writer.abort();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let token = access::startup_token()?;

    let provider: Arc<dyn ProcessProvider> = if args.demo {
        Arc::new(DemoProvider)
    } else {
        match DriverProvider::create(args.decrypt_module.clone()) {
            Ok(p) => Arc::new(p),
            Err(e) => anyhow::bail!("driver interface unavailable ({e:#}); run with --demo to use the simulated process"),
        }
    };

    let project = match &args.project {
        Some(p) if p.exists() => Some(ProjectFile::load(p)?),
        _ if args.demo => {
            let (memory, signatures) = demo_project();
            Some(ProjectFile {
                memory,
                signatures,
                web: Some(json!({ "rootExpr": "[$GWorld]" })),
            })
        }
        _ => None,
    };
    let mut ws = Workspace::new(provider, project, args.project.clone(), args.demo);
    if let Some(pid) = args.pid.or(args.demo.then_some(DEMO_PID)) {
        if let Err(e) = ws.attach(pid) {
            log::warn!("attach to {pid} failed: {e}");
        }
    }

    let (events, _) = broadcast::channel(64);
    let state = AppState {
        ws: Arc::new(Mutex::new(ws)),
        events,
    };

    // Live frames.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(args.tick_ms.max(50)));
            loop {
                interval.tick().await;
                if state.events.receiver_count() == 0 {
                    continue;
                }
                let s = state.clone();
                let frame = tokio::task::spawn_blocking(move || {
                    let mut ws = s.ws.lock().unwrap();
                    (ws.live && ws.is_attached()).then(|| {
                        ws.tick();
                        ws.frame()
                    })
                })
                .await
                .ok()
                .flatten();
                if let Some(f) = frame {
                    state.publish(&f);
                }
            }
        });
    }

    let index = args.static_dir.join("index.html");
    if !index.exists() {
        log::warn!(
            "{} not found; build the UI with `npm run build` in web/app",
            index.display()
        );
    }
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    let bound = listener.local_addr()?;
    let access = access::Access::new(token, bound, args.allowed_origin);
    let app = api_router(state, access)
        .fallback_service(ServeDir::new(&args.static_dir).fallback(ServeFile::new(index)));

    if !args.bind.ip().is_loopback() {
        log::warn!(
            "listening on {}: use a TLS reverse proxy for remote access to protect the API token and process memory",
            args.bind
        );
    }
    log::info!("reclass-server on http://{}", bound);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod access_tests;
