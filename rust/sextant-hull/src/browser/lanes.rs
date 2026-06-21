//! Background async lanes — worker-thread + mpsc-channel infrastructure that
//! keeps disk and LLM work off the UI thread. Extracted from the browser.rs
//! crate root. `PersistenceLane` (Wake + Captain's Log writes/reads) is
//! unconditional; `PilotBrainLane` (local-model planning) is gated on
//! `xilem-shell`. The UI thread submits a request and polls the returned reply
//! channel each frame. `use super::*` supplies the crate root's shared result/
//! record types and imports so the moved bodies are unchanged.

use super::*;

enum PersistenceCommand {
    RecordDistilledPage {
        persona_id: String,
        page: DistilledPage,
        reply_tx: mpsc::Sender<Result<PersistenceDistillResult, String>>,
    },
    SearchWake {
        persona_id: String,
        query: String,
        reply_tx: mpsc::Sender<Result<PersistenceWakeSearchResult, String>>,
    },
    RecordLog {
        entry: LogEntry,
        reply_tx: mpsc::Sender<Result<PersistenceLogResult, String>>,
    },
    RefreshLogs {
        persona_id: String,
        limit: usize,
        reply_tx: mpsc::Sender<Result<PersistenceLogResult, String>>,
    },
    Shutdown,
}

pub(crate) struct PersistenceLane {
    command_tx: mpsc::Sender<PersistenceCommand>,
    worker: Option<thread::JoinHandle<()>>,
}

impl PersistenceLane {
    pub(crate) fn new(data_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        let wake_path = data_dir.join("wake.db");
        let log_path = data_dir.join("captains-log.db");
        let (command_tx, command_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("sextant-persistence-lane".to_string())
            .spawn(move || {
                let wake = match DigitalWake::open(wake_path) {
                    Ok(wake) => wake,
                    Err(error) => {
                        while let Ok(command) = command_rx.recv() {
                            match command {
                                PersistenceCommand::RecordDistilledPage { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Wake initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::SearchWake { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Wake initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::RecordLog { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Wake initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::RefreshLogs { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Wake initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::Shutdown => break,
                            }
                        }
                        return;
                    }
                };
                let log = match CaptainsLog::new(log_path) {
                    Ok(log) => log,
                    Err(error) => {
                        while let Ok(command) = command_rx.recv() {
                            match command {
                                PersistenceCommand::RecordDistilledPage { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Captain's Log initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::SearchWake { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Captain's Log initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::RecordLog { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Captain's Log initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::RefreshLogs { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "Persistence lane Captain's Log initialization failed: {}",
                                        error
                                    )));
                                }
                                PersistenceCommand::Shutdown => break,
                            }
                        }
                        return;
                    }
                };

                while let Ok(command) = command_rx.recv() {
                    match command {
                        PersistenceCommand::RecordDistilledPage {
                            persona_id,
                            page,
                            reply_tx,
                        } => {
                            let result = (|| {
                                wake.record(&persona_id, &page, None)
                                    .map_err(|error| error.to_string())?;
                                let wake_results = wake
                                    .search(&persona_id, &page.title)
                                    .map_err(|error| error.to_string())?;
                                log.record(&LogEntry {
                                    id: Uuid::new_v4(),
                                    timestamp: Utc::now(),
                                    persona_id: persona_id.clone(),
                                    intent: "distill active".to_string(),
                                    plan_json: "{}".to_string(),
                                    signature: "native-browser-shell".to_string(),
                                    consent_signature: None,
                                    status: LogStatus::Success,
                                })
                                .map_err(|error| error.to_string())?;
                                let recent_logs = log
                                    .get_entries(&persona_id, 5)
                                    .map_err(|error| error.to_string())?;
                                Ok(PersistenceDistillResult {
                                    wake_results,
                                    recent_logs,
                                })
                            })();
                            let _ = reply_tx.send(result);
                        }
                        PersistenceCommand::SearchWake {
                            persona_id,
                            query,
                            reply_tx,
                        } => {
                            let result = (|| {
                                let wake_results = wake
                                    .search(&persona_id, &query)
                                    .map_err(|error| error.to_string())?;
                                log.record(&LogEntry {
                                    id: Uuid::new_v4(),
                                    timestamp: Utc::now(),
                                    persona_id: persona_id.clone(),
                                    intent: format!("search Wake '{}'", query),
                                    plan_json: "{}".to_string(),
                                    signature: "native-browser-shell".to_string(),
                                    consent_signature: None,
                                    status: LogStatus::Success,
                                })
                                .map_err(|error| error.to_string())?;
                                let recent_logs = log
                                    .get_entries(&persona_id, 5)
                                    .map_err(|error| error.to_string())?;
                                Ok(PersistenceWakeSearchResult {
                                    wake_results,
                                    recent_logs,
                                })
                            })();
                            let _ = reply_tx.send(result);
                        }
                        PersistenceCommand::RecordLog { entry, reply_tx } => {
                            let result = (|| {
                                let persona_id = entry.persona_id.clone();
                                log.record(&entry).map_err(|error| error.to_string())?;
                                let recent_logs = log
                                    .get_entries(&persona_id, 5)
                                    .map_err(|error| error.to_string())?;
                                Ok(PersistenceLogResult { recent_logs })
                            })();
                            let _ = reply_tx.send(result);
                        }
                        PersistenceCommand::RefreshLogs {
                            persona_id,
                            limit,
                            reply_tx,
                        } => {
                            let result = log
                                .get_entries(&persona_id, limit)
                                .map(|recent_logs| PersistenceLogResult { recent_logs })
                                .map_err(|error| error.to_string());
                            let _ = reply_tx.send(result);
                        }
                        PersistenceCommand::Shutdown => break,
                    }
                }
            })
            .map_err(|error| format!("failed to spawn Persistence lane: {error}"))?;
        Ok(Self {
            command_tx,
            worker: Some(worker),
        })
    }

    pub(crate) fn record_distilled_page(
        &self,
        persona_id: String,
        page: DistilledPage,
    ) -> Result<mpsc::Receiver<Result<PersistenceDistillResult, String>>, String> {
        let (reply_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PersistenceCommand::RecordDistilledPage {
                persona_id,
                page,
                reply_tx,
            })
            .map_err(|error| format!("Persistence lane unavailable: {error}"))?;
        Ok(result_rx)
    }

    pub(crate) fn search_wake(
        &self,
        persona_id: String,
        query: String,
    ) -> Result<mpsc::Receiver<Result<PersistenceWakeSearchResult, String>>, String> {
        let (reply_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PersistenceCommand::SearchWake {
                persona_id,
                query,
                reply_tx,
            })
            .map_err(|error| format!("Persistence lane unavailable: {error}"))?;
        Ok(result_rx)
    }

    pub(crate) fn record_log(
        &self,
        entry: LogEntry,
    ) -> Result<mpsc::Receiver<Result<PersistenceLogResult, String>>, String> {
        let (reply_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PersistenceCommand::RecordLog { entry, reply_tx })
            .map_err(|error| format!("Persistence lane unavailable: {error}"))?;
        Ok(result_rx)
    }

    pub(crate) fn refresh_logs(
        &self,
        persona_id: String,
        limit: usize,
    ) -> Result<mpsc::Receiver<Result<PersistenceLogResult, String>>, String> {
        let (reply_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PersistenceCommand::RefreshLogs {
                persona_id,
                limit,
                reply_tx,
            })
            .map_err(|error| format!("Persistence lane unavailable: {error}"))?;
        Ok(result_rx)
    }
}

impl Drop for PersistenceLane {
    fn drop(&mut self) {
        let _ = self.command_tx.send(PersistenceCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Background lane that turns a natural-language intent into a structured
/// `Vec<PilotAction>` plan using the local model brain. Mirrors `PersistenceLane`:
/// the UI thread sends an intent and polls the reply channel each frame, so the
/// (potentially multi-second) LLM call never blocks rendering or input.
#[cfg(feature = "xilem-shell")]
enum PilotBrainCommand {
    Reason {
        intent: String,
        context: Vec<WakeEntry>,
        reply_tx: mpsc::Sender<Result<Vec<PilotAction>, String>>,
    },
    Shutdown,
}

#[cfg(feature = "xilem-shell")]
pub(crate) struct PilotBrainLane {
    endpoint: String,
    model: String,
    command_tx: mpsc::Sender<PilotBrainCommand>,
    worker: Option<thread::JoinHandle<()>>,
}

#[cfg(feature = "xilem-shell")]
impl PilotBrainLane {
    pub(crate) fn new(config: &AiLocalConfig) -> Self {
        let backend = config.backend_enum();
        let endpoint = config.endpoint.clone();
        let model = config.model.clone();

        let (command_tx, command_rx) = mpsc::channel::<PilotBrainCommand>();
        let worker_endpoint = endpoint.clone();
        let worker_model = model.clone();
        let worker = thread::Builder::new()
            .name("sextant-pilot-brain-lane".to_string())
            .spawn(move || {
                // Build the runtime + brain inside the worker so neither crosses the
                // thread boundary and each reason() call gets a blocking driver.
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        while let Ok(command) = command_rx.recv() {
                            match command {
                                PilotBrainCommand::Reason { reply_tx, .. } => {
                                    let _ = reply_tx.send(Err(format!(
                                        "pilot brain runtime init failed: {error}"
                                    )));
                                }
                                PilotBrainCommand::Shutdown => break,
                            }
                        }
                        return;
                    }
                };
                let brain = match Url::parse(&worker_endpoint) {
                    Ok(url) => Some(LocalBrain::new(backend, url, &worker_model)),
                    Err(error) => {
                        eprintln!("[pilot-brain] invalid endpoint '{worker_endpoint}': {error}");
                        None
                    }
                };
                while let Ok(command) = command_rx.recv() {
                    match command {
                        PilotBrainCommand::Reason {
                            intent,
                            context,
                            reply_tx,
                        } => {
                            let result = match &brain {
                                Some(brain) => runtime.block_on(brain.reason(&intent, &context)),
                                None => Err(format!(
                                    "local model endpoint '{worker_endpoint}' is not a valid URL"
                                )),
                            };
                            let _ = reply_tx.send(result);
                        }
                        PilotBrainCommand::Shutdown => break,
                    }
                }
            })
            .expect("failed to spawn pilot brain lane");
        Self {
            endpoint,
            model,
            command_tx,
            worker: Some(worker),
        }
    }

    pub(crate) fn describe(&self) -> String {
        format!("{} @ {}", self.model, self.endpoint)
    }

    pub(crate) fn reason(
        &self,
        intent: String,
        context: Vec<WakeEntry>,
    ) -> Result<mpsc::Receiver<Result<Vec<PilotAction>, String>>, String> {
        let (reply_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PilotBrainCommand::Reason {
                intent,
                context,
                reply_tx,
            })
            .map_err(|error| format!("pilot brain lane unavailable: {error}"))?;
        Ok(result_rx)
    }
}

#[cfg(feature = "xilem-shell")]
impl Drop for PilotBrainLane {
    fn drop(&mut self) {
        let _ = self.command_tx.send(PilotBrainCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
