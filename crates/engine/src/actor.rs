use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::Instant;
use xilo_models::{CommandError, CommandRequest, Evidence, ModelSpec, Registry};
use xilo_proto::{
    Input, Message, MsgId, Now, Output, ProtoError, Request, Status, TimestampMode, Trace,
    ZxSession,
};

use crate::identity;
use crate::snapshot::{LinkState, Snapshot};
use crate::transport::{Connector, DeviceAddr, Link, LinkEvent};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const ACK_TIMEOUT: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const BLOCKED_RAW: [u16; 1] = [0x0051];

#[derive(Clone, Debug)]
pub struct DeviceSetup {
    pub key: String,
    pub addr: DeviceAddr,
    pub name: Option<String>,
    pub model: Option<String>,
    pub allow_unverified: bool,
    pub auto_connect: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Acked,
    Rejected(Status),
    NoAck,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SendError {
    #[error("the device is not connected")]
    NotConnected,
    #[error("the device model is not known yet")]
    NoModel,
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("this command is blocked in the raw console")]
    Blocked,
    #[error("the device actor stopped")]
    Closed,
}

type Reply = oneshot::Sender<Result<Outcome, SendError>>;

pub(crate) enum Control {
    Send(CommandRequest, Reply),
    SendRaw(Request, Reply),
    Connect,
    Release(Duration),
    SetModel(Option<String>),
    SetAllowUnverified(bool),
    SetName(Option<String>),
    Shutdown,
}

#[derive(Clone)]
pub struct DeviceHandle {
    key: String,
    tx: mpsc::Sender<Control>,
    snapshot: watch::Receiver<Arc<Snapshot>>,
    traces: broadcast::Sender<Trace>,
    tracing_on: Arc<AtomicBool>,
}

impl DeviceHandle {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.borrow().clone()
    }

    pub fn watch(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshot.clone()
    }

    pub async fn send(&self, request: CommandRequest) -> Result<Outcome, SendError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Control::Send(request, reply))
            .await
            .map_err(|_| SendError::Closed)?;
        rx.await.map_err(|_| SendError::Closed)?
    }

    pub async fn send_raw(&self, request: Request) -> Result<Outcome, SendError> {
        if BLOCKED_RAW.contains(&request.id.request().0) {
            return Err(SendError::Blocked);
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Control::SendRaw(request, reply))
            .await
            .map_err(|_| SendError::Closed)?;
        rx.await.map_err(|_| SendError::Closed)?
    }

    pub fn traces(&self) -> broadcast::Receiver<Trace> {
        self.tracing_on.store(true, Ordering::Relaxed);
        self.traces.subscribe()
    }

    pub fn stop_traces(&self) {
        self.tracing_on.store(false, Ordering::Relaxed);
    }

    pub async fn connect(&self) {
        let _ = self.tx.send(Control::Connect).await;
    }

    pub async fn release(&self, duration: Duration) {
        let _ = self.tx.send(Control::Release(duration)).await;
    }

    pub async fn set_model(&self, code: Option<String>) {
        let _ = self.tx.send(Control::SetModel(code)).await;
    }

    pub async fn set_allow_unverified(&self, allow: bool) {
        let _ = self.tx.send(Control::SetAllowUnverified(allow)).await;
    }

    pub async fn set_name(&self, name: Option<String>) {
        let _ = self.tx.send(Control::SetName(name)).await;
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(Control::Shutdown).await;
    }
}

pub(crate) struct Shared {
    pub connector: Arc<dyn Connector>,
    pub registry: Arc<Registry>,
    pub client_id: String,
    pub timestamps: TimestampMode,
}

pub(crate) fn spawn(setup: DeviceSetup, shared: Arc<Shared>) -> DeviceHandle {
    let (tx, rx) = mpsc::channel(64);
    let mut snap = Snapshot::new(setup.key.clone(), setup.addr.clone());
    snap.name = setup.name.clone();
    snap.allow_unverified = setup.allow_unverified;
    snap.model = setup
        .model
        .as_deref()
        .and_then(|c| shared.registry.get(c).cloned());
    let (snap_tx, snap_rx) = watch::channel(Arc::new(snap.clone()));
    let (traces, _) = broadcast::channel(512);
    let tracing_on = Arc::new(AtomicBool::new(false));
    let handle = DeviceHandle {
        key: setup.key.clone(),
        tx,
        snapshot: snap_rx,
        traces: traces.clone(),
        tracing_on: tracing_on.clone(),
    };
    let actor = Actor {
        connect_requested: setup.auto_connect,
        setup,
        shared,
        rx,
        snap,
        snap_tx,
        traces,
        tracing_on,
        queue: VecDeque::new(),
        inflight: None,
        noacks: HashMap::new(),
        released_until: None,
        backoff: Duration::from_secs(2),
        shutdown: false,
        last_keepalive: None,
        last_poll: None,
    };
    tokio::spawn(actor.run());
    handle
}

enum JobKind {
    Command(CommandRequest),
    Raw(Request),
    Internal(String),
}

struct Job {
    kind: JobKind,
    reply: Option<Reply>,
}

impl Job {
    fn label(&self) -> String {
        match &self.kind {
            JobKind::Command(r) => r.command.clone(),
            JobKind::Raw(r) => format!("raw {}", r.id),
            JobKind::Internal(c) => c.clone(),
        }
    }

    fn finish(self, result: Result<Outcome, SendError>) {
        if let Some(reply) = self.reply {
            let _ = reply.send(result);
        }
    }
}

struct Inflight {
    job: Job,
    request: Request,
    sent: Instant,
    retries: u8,
}

enum Event {
    Link(Option<LinkEvent>),
    Control(Option<Control>),
    Tick,
}

struct Actor {
    setup: DeviceSetup,
    shared: Arc<Shared>,
    rx: mpsc::Receiver<Control>,
    snap: Snapshot,
    snap_tx: watch::Sender<Arc<Snapshot>>,
    traces: broadcast::Sender<Trace>,
    tracing_on: Arc<AtomicBool>,
    queue: VecDeque<Job>,
    inflight: Option<Inflight>,
    noacks: HashMap<String, u8>,
    released_until: Option<Instant>,
    backoff: Duration,
    shutdown: bool,
    connect_requested: bool,
    last_keepalive: Option<Instant>,
    last_poll: Option<Instant>,
}

fn now() -> Now {
    Now {
        instant: Instant::now().into_std(),
        unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as u32),
    }
}

impl Actor {
    async fn run(mut self) {
        while !self.shutdown {
            if !self.wait_until_connect().await {
                break;
            }
            self.set_link(LinkState::Connecting);
            let result = tokio::time::timeout(
                CONNECT_TIMEOUT,
                self.shared.connector.connect(&self.setup.addr),
            )
            .await;
            match result {
                Ok(Ok(link)) => {
                    self.backoff = Duration::from_secs(2);
                    self.snap.last_error = None;
                    self.session(link).await;
                }
                Ok(Err(err)) => {
                    tracing::warn!(device = %self.setup.addr, "connect failed: {err}");
                    self.snap.last_error = Some(err.to_string());
                }
                Err(_) => {
                    tracing::warn!(device = %self.setup.addr, "connect timed out");
                    self.snap.last_error = Some("connection timed out".into());
                }
            }
            self.fail_jobs();
            let next = if self.released_until.is_some() {
                LinkState::Released
            } else {
                LinkState::Disconnected
            };
            self.set_link(next);
            if self.shutdown {
                break;
            }
            if self.released_until.is_none() && !self.idle(Some(self.backoff)).await {
                break;
            }
            self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        }
        self.fail_jobs();
    }

    /// Waits until a connection should be attempted. Returns false on shutdown.
    async fn wait_until_connect(&mut self) -> bool {
        loop {
            if let Some(until) = self.released_until {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    self.released_until = None;
                    continue;
                }
                if !self.idle(Some(left)).await {
                    return false;
                }
                continue;
            }
            if self.connect_requested || self.setup.auto_connect {
                return true;
            }
            if !self.idle(None).await {
                return false;
            }
        }
    }

    /// Handles controls while disconnected. Returns false on shutdown.
    async fn idle(&mut self, wait: Option<Duration>) -> bool {
        let deadline = wait.map(|w| Instant::now() + w);
        loop {
            let control = match deadline {
                Some(d) => match tokio::time::timeout_at(d, self.rx.recv()).await {
                    Ok(c) => c,
                    Err(_) => return true,
                },
                None => self.rx.recv().await,
            };
            match control {
                None | Some(Control::Shutdown) => {
                    self.shutdown = true;
                    return false;
                }
                Some(Control::Connect) => {
                    self.connect_requested = true;
                    self.released_until = None;
                    return true;
                }
                Some(Control::Send(_, reply) | Control::SendRaw(_, reply)) => {
                    let _ = reply.send(Err(SendError::NotConnected));
                }
                Some(other) => self.apply_setting(other),
            }
        }
    }

    fn apply_setting(&mut self, control: Control) {
        match control {
            Control::Release(d) => self.released_until = Some(Instant::now() + d),
            Control::SetModel(code) => {
                self.setup.model = code.clone();
                self.snap.model = code.and_then(|c| self.shared.registry.get(&c).cloned());
                self.snap.fields.clear();
                self.snap.unsupported.clear();
                if self.snap.link == LinkState::Ready {
                    self.last_keepalive = None;
                    self.last_poll = None;
                    self.schedule_periodic();
                }
                self.publish();
            }
            Control::SetAllowUnverified(allow) => {
                self.setup.allow_unverified = allow;
                self.snap.allow_unverified = allow;
                self.publish();
            }
            Control::SetName(name) => {
                self.snap.name = name;
                self.publish();
            }
            _ => {}
        }
    }

    async fn session(&mut self, mut link: Box<dyn Link>) {
        let config = identity::session_config(
            &self.shared.client_id,
            self.shared.timestamps,
            link.info().max_write,
        );
        let mut session = ZxSession::new(config);
        self.set_link(LinkState::Handshaking);
        let mut out = Vec::new();
        session.handle(now(), Input::Connected, &mut out);
        if !self.apply(&mut link, out).await {
            link.close().await;
            return;
        }
        let mut tick = tokio::time::interval(TICK);
        loop {
            let event = tokio::select! {
                ev = link.next_event() => Event::Link(ev),
                ctl = self.rx.recv() => Event::Control(ctl),
                _ = tick.tick() => Event::Tick,
            };
            let mut out = Vec::new();
            let mut ticked = false;
            match event {
                Event::Link(Some(LinkEvent::Notify(bytes))) => {
                    session.handle(now(), Input::Notify(&bytes), &mut out);
                }
                Event::Link(Some(LinkEvent::Disconnected(reason))) => {
                    tracing::info!(device = %self.setup.addr, "link closed: {reason}");
                    self.snap.last_error = Some(reason);
                    break;
                }
                Event::Link(None) => break,
                Event::Control(None | Some(Control::Shutdown)) => {
                    self.shutdown = true;
                    link.close().await;
                    return;
                }
                Event::Control(Some(Control::Send(request, reply))) => self.queue.push_back(Job {
                    kind: JobKind::Command(request),
                    reply: Some(reply),
                }),
                Event::Control(Some(Control::SendRaw(request, reply))) => {
                    self.queue.push_back(Job {
                        kind: JobKind::Raw(request),
                        reply: Some(reply),
                    })
                }
                Event::Control(Some(Control::Release(d))) => {
                    self.released_until = Some(Instant::now() + d);
                    link.close().await;
                    return;
                }
                Event::Control(Some(Control::Connect)) => {}
                Event::Control(Some(other)) => self.apply_setting(other),
                Event::Tick => {
                    session.handle(now(), Input::Tick, &mut out);
                    ticked = true;
                }
            }
            if !self.apply(&mut link, out).await {
                break;
            }
            if ticked {
                self.service(&mut link, &mut session).await;
            }
            self.dispatch(&mut link, &mut session).await;
            self.publish_pending();
        }
        link.close().await;
    }

    /// Acts on protocol outputs. Returns false when the session must end.
    async fn apply(&mut self, link: &mut Box<dyn Link>, outputs: Vec<Output>) -> bool {
        let mut queue: VecDeque<Output> = outputs.into();
        while let Some(output) = queue.pop_front() {
            match output {
                Output::Write(bytes) => {
                    if let Err(err) = link.write(&bytes).await {
                        self.snap.last_error = Some(err.to_string());
                        return false;
                    }
                }
                Output::Ready(info) => {
                    tracing::info!(device = %self.setup.addr, serial = ?info.serial, "session ready");
                    self.snap.info = Some(info);
                    self.detect(None);
                    self.set_link(LinkState::Ready);
                    self.last_keepalive = None;
                    self.last_poll = None;
                    self.schedule_periodic();
                }
                Output::Message(message) => self.on_message(message),
                Output::Trace(trace) => {
                    tracing::debug!(
                        device = %self.setup.addr,
                        "{} {} {}",
                        if trace.outgoing { "->" } else { "<-" },
                        trace.cmd.map(|c| c.to_string()).unwrap_or_default(),
                        trace.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    );
                    if self.tracing_on.load(Ordering::Relaxed) {
                        let _ = self.traces.send(trace);
                    }
                }
                Output::Failed(err) => {
                    tracing::warn!(device = %self.setup.addr, "protocol: {err}");
                    self.snap.last_error = Some(err.to_string());
                    if err != ProtoError::NotReady {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn on_message(&mut self, message: Message) {
        let id = message.id();
        self.snap.raw.insert(id, message.payload.clone());
        if let Some(inflight) = &self.inflight
            && id == inflight.request.id.response()
            && let Some(done) = self.inflight.take()
        {
            let outcome = match message.payload.status {
                None | Some(0) => Outcome::Acked,
                Some(code) => Outcome::Rejected(Status::from_code(code)),
            };
            self.noacks.remove(&done.job.label());
            if outcome == Outcome::Acked
                && let JobKind::Command(request) = &done.job.kind
                && let Some(model) = self.snap.model.clone()
                && let Some((field, value)) = model.optimistic(request)
            {
                self.snap.fields.insert(field, value);
            }
            done.job.finish(Ok(outcome));
        }
        if self.snap.model.is_none() {
            self.detect(Some(&message));
        }
        if let Some(model) = self.snap.model.clone() {
            let changed = model.decode(&message, &mut self.snap.fields);
            if !changed.is_empty() || model.is_state_msg(id) {
                self.snap.updated = Some(SystemTime::now());
            }
        }
        self.publish();
    }

    fn detect(&mut self, state: Option<&Message>) {
        if self.snap.model.is_some() {
            return;
        }
        let serial = self.snap.info.as_ref().and_then(|i| i.serial.clone());
        let found = self.shared.registry.detect(&Evidence {
            saved_model: self.setup.model.as_deref(),
            serial: serial.as_deref(),
            state,
        });
        let clear = |i: usize| {
            found
                .get(i + 1)
                .is_none_or(|next| found[i].score > next.score + 0.05)
        };
        if let Some(best) = found.first()
            && (best.reason != "fingerprint" || (best.score >= 0.25 && clear(0)))
        {
            self.snap.model = self.shared.registry.get(&best.code).cloned();
            self.setup.model = Some(best.code.clone());
        }
    }

    fn model(&self) -> Option<Arc<ModelSpec>> {
        self.snap.model.clone()
    }

    fn schedule_periodic(&mut self) {
        let Some(model) = self.model() else {
            self.queue_internal("status_raw");
            return;
        };
        let now = Instant::now();
        let upgrading = self.snap.flag("upgrading");
        if let Some(keepalive) = &model.family.keepalive {
            let due = self
                .last_keepalive
                .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(keepalive.every_s));
            if due && !(keepalive.skip_if.is_some() && upgrading) {
                self.last_keepalive = Some(now);
                self.queue_internal(&keepalive.command.clone());
            }
        }
        if let Some(poll) = &model.family.poll {
            let due = self
                .last_poll
                .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(poll.every_s));
            if due && !(poll.skip_if.is_some() && upgrading) {
                self.last_poll = Some(now);
                self.queue_internal(&poll.command.clone());
            }
        }
    }

    fn queue_internal(&mut self, command: &str) {
        let queued = self
            .queue
            .iter()
            .any(|j| matches!(&j.kind, JobKind::Internal(c) if c == command));
        if !queued {
            self.queue.push_back(Job {
                kind: JobKind::Internal(command.to_string()),
                reply: None,
            });
        }
    }

    async fn service(&mut self, link: &mut Box<dyn Link>, session: &mut ZxSession) {
        if !session.is_ready() {
            return;
        }
        if let Some(inflight) = &mut self.inflight
            && inflight.sent.elapsed() >= ACK_TIMEOUT
        {
            if inflight.retries == 0 {
                inflight.retries = 1;
                inflight.sent = Instant::now();
                let request = inflight.request.clone();
                let mut out = Vec::new();
                session.handle(now(), Input::Send(request), &mut out);
                self.apply(link, out).await;
            } else if let Some(done) = self.inflight.take() {
                let label = done.job.label();
                let misses = self.noacks.entry(label.clone()).or_insert(0);
                *misses += 1;
                if *misses >= 2 {
                    self.snap.unsupported.insert(label);
                }
                done.job.finish(Ok(Outcome::NoAck));
            }
        }
        self.schedule_periodic();
    }

    async fn dispatch(&mut self, link: &mut Box<dyn Link>, session: &mut ZxSession) {
        while self.inflight.is_none() && session.is_ready() {
            let Some(job) = self.queue.pop_front() else {
                return;
            };
            let request = match self.build(&job) {
                Ok(request) => request,
                Err(err) => {
                    job.finish(Err(err));
                    continue;
                }
            };
            let mut out = Vec::new();
            session.handle(now(), Input::Send(request.clone()), &mut out);
            self.inflight = Some(Inflight {
                job,
                request,
                sent: Instant::now(),
                retries: 0,
            });
            if !self.apply(link, out).await {
                return;
            }
        }
    }

    fn build(&self, job: &Job) -> Result<Request, SendError> {
        match &job.kind {
            JobKind::Raw(request) => Ok(request.clone()),
            JobKind::Internal(command) if command == "status_raw" => Ok(Request {
                id: MsgId(0x0040),
                params: Vec::new(),
            }),
            JobKind::Internal(command) => {
                let model = self.model().ok_or(SendError::NoModel)?;
                Ok(model.build(
                    &CommandRequest::new(command.clone(), None),
                    &self.snap.fields,
                    self.setup.allow_unverified,
                )?)
            }
            JobKind::Command(request) => {
                let model = self.model().ok_or(SendError::NoModel)?;
                Ok(model.build(request, &self.snap.fields, self.setup.allow_unverified)?)
            }
        }
    }

    fn fail_jobs(&mut self) {
        if let Some(inflight) = self.inflight.take() {
            inflight.job.finish(Err(SendError::NotConnected));
        }
        for job in self.queue.drain(..) {
            job.finish(Err(SendError::NotConnected));
        }
        self.snap.pending.clear();
    }

    fn set_link(&mut self, state: LinkState) {
        self.snap.link = state;
        self.publish();
    }

    fn publish_pending(&mut self) {
        let pending = self
            .inflight
            .iter()
            .map(|i| &i.job)
            .chain(self.queue.iter())
            .filter(|j| matches!(j.kind, JobKind::Command(_)))
            .map(Job::label)
            .collect();
        if pending != self.snap.pending {
            self.snap.pending = pending;
            self.publish();
        }
    }

    fn publish(&mut self) {
        self.snap_tx.send_replace(Arc::new(self.snap.clone()));
    }
}
