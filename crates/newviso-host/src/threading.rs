use super::*;
use std::{
    cmp::Ordering as CmpOrdering,
    collections::{BTreeMap, BinaryHeap},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Condvar,
    },
    thread::{self, JoinHandle},
};

const THREADING_SERVICE_ID: &str = "threading.api";
const ENGINE_THREADING_GATEWAY: &str = "engine.threading";
const MAX_EXTERNAL_PROCESSES: usize = 2;

#[derive(Clone, Debug)]
struct ServiceCallJob {
    task_id: String,
    name: String,
    lane: String,
    priority_rank: u8,
    seq: u64,
    gateway: String,
    method: String,
    payload: serde_json::Value,
}

#[derive(Clone, Debug)]
struct QueueEntry(ServiceCallJob);

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.priority_rank == other.0.priority_rank && self.0.seq == other.0.seq
    }
}
impl Eq for QueueEntry {}
impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.0
            .priority_rank
            .cmp(&other.0.priority_rank)
            // Older jobs win inside the same priority.
            .then_with(|| other.0.seq.cmp(&self.0.seq))
    }
}

#[derive(Clone, Debug)]
struct JobStatus {
    name: String,
    lane: String,
    priority: String,
    phase: &'static str,
    detail: String,
}

struct ThreadingShared {
    queue: Mutex<BinaryHeap<QueueEntry>>,
    wake: Condvar,
    statuses: Mutex<BTreeMap<String, JobStatus>>,
    process_results: Mutex<BTreeMap<String, Vec<u8>>>,
    process_active: Mutex<usize>,
    process_wake: Condvar,
    process_waiting: AtomicU64,
    stopping: AtomicBool,
    seq: AtomicU64,
    submitted: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    running: AtomicU64,
}

impl Default for ThreadingShared {
    fn default() -> Self {
        Self {
            queue: Mutex::new(BinaryHeap::new()),
            wake: Condvar::new(),
            statuses: Mutex::new(BTreeMap::new()),
            process_results: Mutex::new(BTreeMap::new()),
            process_active: Mutex::new(0),
            process_wake: Condvar::new(),
            process_waiting: AtomicU64::new(0),
            stopping: AtomicBool::new(false),
            seq: AtomicU64::new(1),
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            running: AtomicU64::new(0),
        }
    }
}

struct ThreadingService {
    shared: Arc<ThreadingShared>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    process_workers: Mutex<Vec<JoinHandle<()>>>,
    worker_count: usize,
}

impl ThreadingService {
    fn new() -> Self {
        let worker_count = std::thread::available_parallelism()
            .map(|count| count.get().saturating_sub(1).clamp(2, 8))
            .unwrap_or(4);
        let shared = Arc::new(ThreadingShared::default());
        let workers = (0..worker_count)
            .map(|index| {
                let shared = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("newviso-job-{index}"))
                    .spawn(move || worker_loop(shared))
                    .expect("failed to spawn NewViso job worker")
            })
            .collect::<Vec<_>>();
        Self {
            shared,
            workers: Mutex::new(workers),
            process_workers: Mutex::new(Vec::new()),
            worker_count,
        }
    }

    fn encode(value: serde_json::Value) -> RResult<Blob, RString> {
        match serde_json::to_vec(&value) {
            Ok(bytes) => RResult::ROk(Blob::from(bytes)),
            Err(error) => RResult::RErr(RString::from(error.to_string())),
        }
    }

    fn priority_rank(priority: &str) -> u8 {
        match priority.trim().to_ascii_lowercase().as_str() {
            "critical" => 5,
            "interactive" | "high" => 4,
            "normal" => 3,
            "low" => 2,
            "background" => 1,
            _ => 3,
        }
    }

    fn submit_service_call(&self, value: serde_json::Value) -> RResult<Blob, RString> {
        let target = value.get("target").and_then(serde_json::Value::as_object);
        let gateway = target
            .and_then(|target| target.get("gateway"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let method = target
            .and_then(|target| target.get("method"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if gateway.is_empty() || method.is_empty() {
            return RResult::RErr(RString::from(
                "engine.threading job.invoke_service_v1 requires target.gateway and target.method",
            ));
        }

        let seq = self.shared.seq.fetch_add(1, Ordering::Relaxed);
        let task_id = value
            .get("task_id")
            .or_else(|| value.get("job_id"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("newviso.job.{seq}"));
        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("service call")
            .to_owned();
        let lane = value
            .get("lane")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("plugin")
            .to_owned();
        let priority = value
            .get("priority")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("normal")
            .to_owned();
        let payload = target
            .and_then(|target| target.get("payload_json"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        self.shared
            .statuses
            .lock()
            .expect("threading status map poisoned")
            .insert(
                task_id.clone(),
                JobStatus {
                    name: name.clone(),
                    lane: lane.clone(),
                    priority: priority.clone(),
                    phase: "Scheduled",
                    detail: "queued".to_owned(),
                },
            );

        self.shared
            .queue
            .lock()
            .expect("threading queue poisoned")
            .push(QueueEntry(ServiceCallJob {
                task_id: task_id.clone(),
                name,
                lane,
                priority_rank: Self::priority_rank(&priority),
                seq,
                gateway: gateway.clone(),
                method: method.clone(),
                payload,
            }));
        self.shared.submitted.fetch_add(1, Ordering::Relaxed);
        self.shared.wake.notify_one();

        Self::encode(serde_json::json!({
            "task_id": task_id,
            "job_id": task_id,
            "accepted": true,
            "gateway": gateway,
            "method": method,
            "status": "scheduled",
            "detail": "service call queued on NewViso engine.threading"
        }))
    }

    fn start_process(&self, value: serde_json::Value) -> RResult<Blob, RString> {
        let request: newviso_task_api::TaskRunProcessStartRequestV1 =
            match serde_json::from_value(value) {
                Ok(request) => request,
                Err(error) => {
                    return RResult::RErr(RString::from(format!(
                        "engine.threading task.run_process_start_v1 invalid request: {error}"
                    )))
                }
            };

        let task_id = if request.task_id.trim().is_empty() {
            let seq = self.shared.seq.fetch_add(1, Ordering::Relaxed);
            format!("newviso.process.{seq}")
        } else {
            request.task_id.clone()
        };
        if request.executable.trim().is_empty() {
            return RResult::RErr(RString::from(
                "engine.threading process task requires executable",
            ));
        }

        {
            let mut statuses = self
                .shared
                .statuses
                .lock()
                .expect("threading status map poisoned");
            if statuses
                .get(&task_id)
                .is_some_and(|status| !matches!(status.phase, "Completed" | "Failed" | "Cancelled"))
            {
                return Self::encode(serde_json::json!({
                    "task_id": task_id,
                    "job_id": task_id,
                    "accepted": false,
                    "status": "rejected",
                    "detail": "task id already active",
                    "result_path": request.result_path,
                }));
            }
            statuses.insert(
                task_id.clone(),
                JobStatus {
                    name: request.name.clone(),
                    lane: request.lane.clone(),
                    priority: request.priority.clone(),
                    phase: "Scheduled",
                    detail: format!("process queued executable={}", request.executable),
                },
            );
        }
        self.shared
            .process_results
            .lock()
            .expect("threading process result map poisoned")
            .remove(&task_id);
        self.shared.submitted.fetch_add(1, Ordering::Relaxed);

        let shared = Arc::clone(&self.shared);
        let worker_task_id = task_id.clone();
        let executable = request.executable.clone();
        let args = request.args.clone();
        let cwd = request.cwd.clone();
        let env = request.env.clone();
        let result_path = request.result_path.clone();

        let spawn = thread::Builder::new()
            .name(format!("newviso-process-{}", worker_task_id))
            .spawn(move || {
                let mut active = shared
                    .process_active
                    .lock()
                    .expect("threading process gate poisoned");
                if *active >= MAX_EXTERNAL_PROCESSES {
                    shared.process_waiting.fetch_add(1, Ordering::Relaxed);
                    if let Some(status) = shared
                        .statuses
                        .lock()
                        .expect("threading status map poisoned")
                        .get_mut(&worker_task_id)
                    {
                        status.phase = "Blocked";
                        status.detail = format!(
                            "waiting for external process slot max={MAX_EXTERNAL_PROCESSES}"
                        );
                    }
                    while *active >= MAX_EXTERNAL_PROCESSES
                        && !shared.stopping.load(Ordering::Acquire)
                    {
                        active = shared
                            .process_wake
                            .wait(active)
                            .expect("threading process gate poisoned");
                    }
                    shared.process_waiting.fetch_sub(1, Ordering::Relaxed);
                }
                if shared.stopping.load(Ordering::Acquire) {
                    drop(active);
                    mark_process_failed(
                        &shared,
                        &worker_task_id,
                        "host shutdown before external process acquired a slot".to_owned(),
                    );
                    return;
                }
                *active += 1;
                drop(active);
                let _process_slot = ProcessSlotGuard { shared: &shared };

                shared.running.fetch_add(1, Ordering::Relaxed);
                if let Some(status) = shared
                    .statuses
                    .lock()
                    .expect("threading status map poisoned")
                    .get_mut(&worker_task_id)
                {
                    status.phase = "Running";
                    status.detail = format!("process running executable={executable}");
                }

                let mut command = std::process::Command::new(&executable);
                command.args(&args);
                if !cwd.trim().is_empty() {
                    command.current_dir(&cwd);
                }
                command.envs(&env);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                    command.creation_flags(CREATE_NO_WINDOW);
                }

                let output = command.output();
                shared.running.fetch_sub(1, Ordering::Relaxed);

                let outcome = match output {
                    Ok(output) if output.status.success() => {
                        let bytes = if result_path.trim().is_empty() {
                            output.stdout
                        } else {
                            match std::fs::read(&result_path) {
                                Ok(bytes) => bytes,
                                Err(error) => {
                                    let detail = format!(
                                        "process succeeded but result read failed path='{}': {}",
                                        result_path, error
                                    );
                                    mark_process_failed(&shared, &worker_task_id, detail);
                                    return;
                                }
                            }
                        };
                        shared
                            .process_results
                            .lock()
                            .expect("threading process result map poisoned")
                            .insert(worker_task_id.clone(), bytes);
                        Ok(format!(
                            "process completed executable={} result_path={}",
                            executable, result_path
                        ))
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stderr = stderr.trim();
                        let stderr = stderr.chars().take(2048).collect::<String>();
                        Err(format!(
                            "process exited status={} executable={} stderr={}",
                            output.status, executable, stderr
                        ))
                    }
                    Err(error) => Err(format!(
                        "process spawn failed executable={} error={}",
                        executable, error
                    )),
                };

                match outcome {
                    Ok(detail) => {
                        shared.completed.fetch_add(1, Ordering::Relaxed);
                        if let Some(status) = shared
                            .statuses
                            .lock()
                            .expect("threading status map poisoned")
                            .get_mut(&worker_task_id)
                        {
                            status.phase = "Completed";
                            status.detail = detail;
                        }
                    }
                    Err(detail) => mark_process_failed(&shared, &worker_task_id, detail),
                }
            });

        let handle = match spawn {
            Ok(handle) => handle,
            Err(error) => {
                mark_process_failed(
                    &self.shared,
                    &task_id,
                    format!("failed to spawn process worker thread: {error}"),
                );
                return RResult::RErr(RString::from(format!(
                    "engine.threading failed to create process worker: {error}"
                )));
            }
        };
        {
            let mut process_workers = self
                .process_workers
                .lock()
                .expect("threading process worker list poisoned");
            let mut index = 0;
            while index < process_workers.len() {
                if process_workers[index].is_finished() {
                    let finished = process_workers.swap_remove(index);
                    let _ = finished.join();
                } else {
                    index += 1;
                }
            }
            process_workers.push(handle);
        }

        Self::encode(serde_json::json!({
            "task_id": task_id,
            "job_id": task_id,
            "accepted": true,
            "status": "scheduled",
            "detail": "external process queued on NewViso engine.threading",
            "result_path": request.result_path,
        }))
    }

    fn result_bin(&self, value: serde_json::Value) -> RResult<Blob, RString> {
        let task_id = value
            .get("task_id")
            .or_else(|| value.get("job_id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if task_id.is_empty() {
            return RResult::RErr(RString::from(
                "engine.threading task.result_bin_v1 requires task_id",
            ));
        }

        if let Some(bytes) = self
            .shared
            .process_results
            .lock()
            .expect("threading process result map poisoned")
            .remove(task_id)
        {
            return RResult::ROk(Blob::from(bytes));
        }

        let status = self
            .shared
            .statuses
            .lock()
            .expect("threading status map poisoned")
            .get(task_id)
            .cloned();
        match status {
            Some(status) => RResult::RErr(RString::from(format!(
                "engine.threading process result unavailable task_id='{}' phase={} detail={}",
                task_id, status.phase, status.detail
            ))),
            None => RResult::RErr(RString::from(format!(
                "engine.threading process task not found task_id='{task_id}'"
            ))),
        }
    }

    fn status(&self, value: serde_json::Value) -> RResult<Blob, RString> {
        let task_id = value
            .get("task_id")
            .or_else(|| value.get("job_id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let status = self
            .shared
            .statuses
            .lock()
            .expect("threading status map poisoned")
            .get(task_id)
            .cloned();
        let value = if let Some(status) = status {
            serde_json::json!({
                "task_id": task_id,
                "job_id": task_id,
                "name": status.name,
                "lane": status.lane,
                "priority": status.priority,
                "phase": status.phase,
                "found": true,
                "detail": status.detail,
                "can_pause": false,
                "can_cancel": false,
                "cancel_requested": false,
                "pause_requested": false
            })
        } else {
            serde_json::json!({
                "task_id": task_id,
                "job_id": task_id,
                "phase": "Completed",
                "found": false
            })
        };
        Self::encode(value)
    }

    fn snapshot(&self) -> RResult<Blob, RString> {
        let pending = self
            .shared
            .queue
            .lock()
            .expect("threading queue poisoned")
            .len();
        Self::encode(serde_json::json!({
            "worker_threads": self.worker_count,
            "pending_threading": pending,
            "running_threading": self.shared.running.load(Ordering::Relaxed),
            "running_external_processes": *self.shared.process_active.lock().expect("threading process gate poisoned"),
            "pending_external_processes": self.shared.process_waiting.load(Ordering::Relaxed),
            "max_external_processes": MAX_EXTERNAL_PROCESSES,
            "submitted_threading": self.shared.submitted.load(Ordering::Relaxed),
            "completed_threading": self.shared.completed.load(Ordering::Relaxed),
            "failed_threading": self.shared.failed.load(Ordering::Relaxed),
            "provider": "newviso.host.threading",
            "gateway": ENGINE_THREADING_GATEWAY
        }))
    }
}

impl ServiceV1 for ThreadingService {
    fn id(&self) -> CapabilityId {
        CapabilityId::from(THREADING_SERVICE_ID)
    }

    fn describe(&self) -> RString {
        RString::from(
            serde_json::json!({
                "service": THREADING_SERVICE_ID,
                "engine_gateway": ENGINE_THREADING_GATEWAY,
                "contract": "newengine.threading.runtime.v1",
                "ownership": "NewViso host",
                "methods": [
                    "task.invoke_service_v1",
                    "task.run_process_start_v1",
                    "task.result_bin_v1",
                    "task.status_json_v1",
                    "task.snapshot_json_v1",
                    "job.invoke_service_v1",
                    "job.status_json_v1",
                    "job.snapshot_json_v1",
                    "info_json"
                ]
            })
            .to_string(),
        )
    }

    fn call(&self, method: MethodName, payload: Blob) -> RResult<Blob, RString> {
        let value = if payload.is_empty() {
            serde_json::Value::Null
        } else {
            match serde_json::from_slice(payload.as_slice()) {
                Ok(value) => value,
                Err(error) => {
                    return RResult::RErr(RString::from(format!(
                        "engine.threading invalid JSON: {error}"
                    )))
                }
            }
        };

        match method.as_str() {
            "task.run_process_start_v1" | "job.run_process_start_v1" => self.start_process(value),
            "task.result_bin_v1" | "job.result_bin_v1" => self.result_bin(value),
            "task.invoke_service_v1" | "job.invoke_service_v1" | "threading.invoke_service_v1" => {
                self.submit_service_call(value)
            }
            "task.status_json_v1" | "job.status_json_v1" | "threading.status_json_v1" => {
                self.status(value)
            }
            "task.snapshot_json_v1" | "job.snapshot_json_v1" | "threading.snapshot_json_v1" => {
                self.snapshot()
            }
            "info_json" => Self::encode(serde_json::json!({
                "service": THREADING_SERVICE_ID,
                "gateway": ENGINE_THREADING_GATEWAY,
                "worker_threads": self.worker_count
            })),
            "shutdown_v1" => RResult::ROk(Blob::new()),
            other => RResult::RErr(RString::from(format!(
                "engine.threading unsupported method '{other}'"
            ))),
        }
    }
}

impl Drop for ThreadingService {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        self.shared.wake.notify_all();
        self.shared.process_wake.notify_all();

        let workers =
            std::mem::take(&mut *self.workers.lock().expect("threading worker list poisoned"));
        for worker in workers {
            let _ = worker.join();
        }

        let process_workers = std::mem::take(
            &mut *self
                .process_workers
                .lock()
                .expect("threading process worker list poisoned"),
        );
        for worker in process_workers {
            let _ = worker.join();
        }
    }
}

struct ProcessSlotGuard<'a> {
    shared: &'a ThreadingShared,
}

impl Drop for ProcessSlotGuard<'_> {
    fn drop(&mut self) {
        let mut active = self
            .shared
            .process_active
            .lock()
            .expect("threading process gate poisoned");
        *active = active.saturating_sub(1);
        drop(active);
        self.shared.process_wake.notify_one();
    }
}

fn mark_process_failed(shared: &ThreadingShared, task_id: &str, detail: String) {
    shared.failed.fetch_add(1, Ordering::Relaxed);
    if let Some(status) = shared
        .statuses
        .lock()
        .expect("threading status map poisoned")
        .get_mut(task_id)
    {
        status.phase = "Failed";
        status.detail = detail.clone();
    }
    warn(
        "newviso.threading",
        format!("process task '{}' failed: {}", task_id, detail),
    );
}

fn worker_loop(shared: Arc<ThreadingShared>) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().expect("threading queue poisoned");
            loop {
                if let Some(job) = queue.pop() {
                    break Some(job.0);
                }
                if shared.stopping.load(Ordering::Acquire) {
                    break None;
                }
                queue = shared.wake.wait(queue).expect("threading queue poisoned");
            }
        };

        let Some(job) = job else {
            return;
        };
        shared.running.fetch_add(1, Ordering::Relaxed);
        if let Some(status) = shared
            .statuses
            .lock()
            .expect("threading status map poisoned")
            .get_mut(&job.task_id)
        {
            status.phase = "Running";
            status.detail = format!(
                "worker executing {} / {} lane={} name={}",
                job.gateway, job.method, job.lane, job.name
            );
        }

        let result = call_json(&job.gateway, &job.method, &job.payload);
        shared.running.fetch_sub(1, Ordering::Relaxed);
        match result {
            Ok(_) => {
                shared.completed.fetch_add(1, Ordering::Relaxed);
                if let Some(status) = shared
                    .statuses
                    .lock()
                    .expect("threading status map poisoned")
                    .get_mut(&job.task_id)
                {
                    status.phase = "Completed";
                    status.detail = "completed".to_owned();
                }
            }
            Err(error) => {
                shared.failed.fetch_add(1, Ordering::Relaxed);
                if let Some(status) = shared
                    .statuses
                    .lock()
                    .expect("threading status map poisoned")
                    .get_mut(&job.task_id)
                {
                    status.phase = "Failed";
                    status.detail = error.clone();
                }
                warn(
                    "newviso.threading",
                    format!(
                        "job '{}' target={}/{} failed: {}",
                        job.task_id, job.gateway, job.method, error
                    ),
                );
            }
        }
    }
}

pub fn ensure_threading_service() -> Result<(), String> {
    {
        let host = state().read().expect("NewViso host state poisoned");
        if host.services.contains_key(THREADING_SERVICE_ID) {
            return Ok(());
        }
    }

    let service: ServiceV1Dyn<'static> =
        ServiceV1_TO::from_value(ThreadingService::new(), TD_Opaque);
    match register_service_v1(service) {
        RResult::ROk(()) => {
            info(
                "newviso.threading",
                format!(
                    "host job system registered service='{}' gateway='{}'",
                    THREADING_SERVICE_ID, ENGINE_THREADING_GATEWAY
                ),
            );
            Ok(())
        }
        RResult::RErr(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_task_api::{TaskRunProcessStartRequestV1, TaskRunProcessStartedV1};
    use std::time::{Duration, Instant};

    fn unwrap_blob(result: RResult<Blob, RString>) -> Vec<u8> {
        result
            .into_result()
            .unwrap_or_else(|error| panic!("threading service call failed: {error}"))
            .into_vec()
    }

    #[test]
    fn external_process_completes_and_result_is_one_shot() {
        let service = ThreadingService::new();
        let executable = std::env::current_exe()
            .expect("test executable path")
            .display()
            .to_string();
        let request = TaskRunProcessStartRequestV1 {
            task_id: "test.external-process".to_owned(),
            name: "host process contract test".to_owned(),
            owner: "newviso-host-test".to_owned(),
            category: "test".to_owned(),
            lane: "background".to_owned(),
            priority: "background".to_owned(),
            executable,
            args: vec!["--list".to_owned()],
            result_path: String::new(),
            can_cancel: false,
            ..Default::default()
        };

        let started: TaskRunProcessStartedV1 = serde_json::from_slice(&unwrap_blob(
            service.start_process(serde_json::to_value(&request).expect("request json")),
        ))
        .expect("started response");
        assert!(started.accepted);

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status: serde_json::Value = serde_json::from_slice(&unwrap_blob(
                service.status(serde_json::json!({"task_id": started.task_id})),
            ))
            .expect("status response");
            match status.get("phase").and_then(serde_json::Value::as_str) {
                Some("Completed") => break,
                Some("Failed") => panic!(
                    "external process failed: {}",
                    status
                        .get("detail")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("<no detail>")
                ),
                _ if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                other => panic!("external process did not complete, phase={other:?}"),
            }
        }

        let result = unwrap_blob(service.result_bin(serde_json::json!({
            "task_id": started.task_id
        })));
        assert!(
            !result.is_empty(),
            "test process should return the libtest --list output"
        );

        assert!(
            service
                .result_bin(serde_json::json!({"task_id": "test.external-process"}))
                .into_result()
                .is_err(),
            "binary process result must be consumed exactly once"
        );
    }
}
