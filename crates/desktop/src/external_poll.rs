//! Bounded HTTP projection of an existing stream for enrolled native clients.
//! View handles grant no owner APIs; expiration closes only the upstream socket.
use crate::{Api, Shared, external};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
#[derive(Default)]
pub struct Views(pub Mutex<HashMap<String, Arc<Mutex<View>>>>);
pub struct View {
    device: String,
    session: String,
    last: Instant,
    next: u64,
    bytes: usize,
    frames: VecDeque<(u64, Value, usize)>,
    tx: mpsc::Sender<Message>,
    closed: bool,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Open,
    Read {
        view: String,
        after: u64,
    },
    Send {
        view: String,
        data: Option<String>,
        control: Option<Value>,
    },
    Close {
        view: String,
    },
}
fn bad() -> (StatusCode, &'static str) {
    (StatusCode::BAD_REQUEST, "Invalid or expired terminal view")
}
pub async fn call(
    State(app): State<Shared>,
    Path((device, session)): Path<(String, String)>,
    Json(op): Json<Operation>,
) -> Api {
    if !external::valid_session(&session) {
        return Err(bad());
    }
    if matches!(op, Operation::Open) {
        let h = external::host(&app, &device)?;
        {
            let mut views = app.external_views.0.lock().map_err(|_| bad())?;
            views.retain(|_, v| {
                v.lock()
                    .is_ok_and(|v| v.last.elapsed() < Duration::from_secs(30) && !v.closed)
            });
            if views.len() >= 32 {
                return Err((StatusCode::TOO_MANY_REQUESTS, "Too many terminal views"));
            }
        }
        let mut req = format!(
            "ws://{}/sessions/{session}/stream",
            h.url.trim_start_matches("http://").trim_end_matches('/')
        )
        .into_client_request()
        .map_err(|_| bad())?;
        req.headers_mut().insert(
            "Cookie",
            format!("axxisd_session={}", h.token)
                .parse()
                .map_err(|_| bad())?,
        );
        let (ws, _) = tokio::time::timeout(
            Duration::from_secs(8),
            tokio_tungstenite::connect_async(req),
        )
        .await
        .map_err(|_| bad())?
        .map_err(|_| bad())?;
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random)
            .map_err(|_| bad())?;
        let id = hex::encode(random);
        let (tx, mut rx) = mpsc::channel(32);
        let view = Arc::new(Mutex::new(View {
            device,
            session,
            last: Instant::now(),
            next: 0,
            bytes: 0,
            frames: VecDeque::new(),
            tx,
            closed: false,
        }));
        {
            let mut views = app.external_views.0.lock().map_err(|_| bad())?;
            if views.len() >= 32 {
                return Err((StatusCode::TOO_MANY_REQUESTS, "Too many terminal views"));
            }
            views.insert(id.clone(), view.clone());
        }
        tokio::spawn(async move {
            let (mut sink, mut stream) = ws.split();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                 _=tick.tick()=>{if view.lock().map_or(true,|v|v.closed||v.last.elapsed()>Duration::from_secs(30)){break;}},
                 msg=rx.recv()=>{let Some(msg)=msg else{break};if !matches!(tokio::time::timeout(Duration::from_secs(3),sink.send(msg)).await,Ok(Ok(()))){break;}},
                 msg=stream.next()=>{
                  let frames:Vec<(Value,usize)>=match msg{Some(Ok(Message::Binary(b)))=>b.chunks(32768).map(|b|(json!({"data":hex::encode(b)}),b.len())).collect(),Some(Ok(Message::Text(t)))if t.len()<=32768=>vec![(json!({"control":t.as_str()}),t.len())],Some(Ok(Message::Ping(b)))=>{if sink.send(Message::Pong(b)).await.is_err(){break;}continue;},Some(Ok(Message::Pong(_)))=>continue,_=>break};
                  let Ok(mut v)=view.lock()else{break};if v.bytes+frames.iter().map(|f|f.1).sum::<usize>()>4*1024*1024{break;}for(frame,size)in frames{v.next+=1;let seq=v.next;v.frames.push_back((seq,frame,size));v.bytes+=size;}
                 }
                }
            }
            if let Ok(mut v) = view.lock() {
                v.closed = true;
            }
            let _ = sink.close().await;
        });
        return Ok(Json(json!({"view":id})));
    }
    let id = match &op {
        Operation::Read { view, .. } | Operation::Send { view, .. } | Operation::Close { view } => {
            view
        }
        _ => unreachable!(),
    };
    let view = app
        .external_views
        .0
        .lock()
        .map_err(|_| bad())?
        .get(id)
        .cloned()
        .ok_or_else(bad)?;
    let mut v = view.lock().map_err(|_| bad())?;
    if v.device != device || v.session != session || v.last.elapsed() > Duration::from_secs(30) {
        return Err(bad());
    }
    v.last = Instant::now();
    match op {
        Operation::Read { after, .. } => {
            if after > v.next {
                return Err(bad());
            }
            while v.frames.front().is_some_and(|f| f.0 <= after) {
                let (_, _, n) = v.frames.pop_front().unwrap();
                v.bytes -= n;
            }
            let mut size = 0;
            let frames: Vec<_> = v
                .frames
                .iter()
                .take_while(|f| {
                    size += f.2;
                    size <= 128 * 1024
                })
                .map(|(seq, frame, _)| json!({"seq":seq,"frame":frame}))
                .collect();
            Ok(Json(json!({"frames":frames,"closed":v.closed})))
        }
        Operation::Send { data, control, .. } => {
            if v.closed {
                return Err(bad());
            }
            let message = match (data, control) {
                (Some(data), None) if data.len() <= 65536 => {
                    Message::Binary(hex::decode(data).map_err(|_| bad())?.into())
                }
                (None, Some(c))
                    if matches!(
                        c["type"].as_str(),
                        Some(
                            "resize"
                                | "claim_resize"
                                | "release_resize"
                                | "pause"
                                | "resume"
                                | "ping"
                        )
                    ) =>
                {
                    Message::Text(c.to_string().into())
                }
                _ => return Err(bad()),
            };
            v.tx.try_send(message)
                .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Input was not queued"))?;
            Ok(Json(json!({"queued":true})))
        }
        Operation::Close { .. } => {
            v.closed = true;
            Ok(Json(json!({"closed":true})))
        }
        Operation::Open => unreachable!(),
    }
}
