//! Persistent skinning worker pool and immutable worker jobs.
use super::*;

pub(in crate::first_scene) struct SkinningWorkerPool {
    task_tx: Option<Sender<SkinningJob>>,
    result_rx: Receiver<SkinningCompletion>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for SkinningWorkerPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SkinningWorkerPool")
            .field("worker_count", &self.workers.len())
            .finish_non_exhaustive()
    }
}

impl SkinningWorkerPool {
    pub(in crate::first_scene) fn new() -> Result<Self, String> {
        let (task_tx, task_rx) = mpsc::channel::<SkinningJob>();
        let (result_tx, result_rx) = mpsc::channel::<SkinningCompletion>();
        let task_rx = Arc::new(Mutex::new(task_rx));
        let mut workers = Vec::with_capacity(SKINNING_WORKER_COUNT);

        for index in 0..SKINNING_WORKER_COUNT {
            let task_rx = Arc::clone(&task_rx);
            let result_tx = result_tx.clone();
            let worker = thread::Builder::new()
                .name(format!("newviso-skin-{index}"))
                .spawn(move || loop {
                    let job = {
                        let receiver = task_rx.lock().expect("skinning task queue poisoned");
                        receiver.recv()
                    };
                    let Ok(job) = job else {
                        return;
                    };
                    let stable_id = job.stable_id;
                    let completion = match run_skinning_job(job) {
                        Ok(result) => SkinningCompletion::Ready(result),
                        Err(error) => SkinningCompletion::Failed { stable_id, error },
                    };
                    if result_tx.send(completion).is_err() {
                        return;
                    }
                })
                .map_err(|error| format!("failed to spawn skinning worker: {error}"))?;
            workers.push(worker);
        }

        Ok(Self {
            task_tx: Some(task_tx),
            result_rx,
            workers,
        })
    }

    pub(super) fn submit(&self, job: SkinningJob) -> Result<(), String> {
        self.task_tx
            .as_ref()
            .ok_or_else(|| "skinning worker pool is shutting down".to_owned())?
            .send(job)
            .map_err(|_| "skinning worker pool disconnected".to_owned())
    }

    pub(super) fn try_recv(&self) -> Result<SkinningCompletion, TryRecvError> {
        self.result_rx.try_recv()
    }
}

impl Drop for SkinningWorkerPool {
    fn drop(&mut self) {
        self.task_tx.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}


fn run_skinning_job(job: SkinningJob) -> Result<SkinningResult, String> {
    let mut palette = if let Some(transition) = job.transition_from.as_ref() {
        build_blended_skin_palette(
            &job.skeleton,
            &job.derived_joint_drivers,
            &transition.clip,
            transition.sample_time,
            &job.clip,
            job.sample_time,
            transition.alpha,
            job.source_to_model,
            job.model_to_source,
        )?
    } else {
        build_skin_palette(
            &job.skeleton,
            &job.derived_joint_drivers,
            &job.clip,
            job.sample_time,
            job.source_to_model,
            job.model_to_source,
        )?
    };
    let physical = job.pose_layers.is_physical();
    palette = pose_layers::apply_layers(
        palette,
        &job.pose_layers,
        &job.skeleton,
        &job.derived_joint_drivers,
        job.source_to_model,
        job.model_to_source,
    )?;
    if !physical {
        arm_ik::apply_to_palette(
            &mut palette,
            &job.skeleton,
            &job.derived_joint_drivers,
            job.source_to_model,
            &job.arm_ik,
        )?;
    }
    let feet = job.foot_joint_indices
        .and_then(|indices| skin_palette_feet_positions(
            &job.skeleton, &palette, job.source_to_model, indices,
        ));
    let packed = if job.gpu_skinning { Vec::new() } else {
        skin_vertices_to_packed(&job.bind_vertices, &palette)?
    };
    if !job.gpu_skinning && packed.len() != job.expected_vertex_count * FLOATS_PER_VERTEX {
        return Err(format!(
            "skinned packed vertex count changed entity={} actual_floats={} expected_floats={}",
            job.stable_id,
            packed.len(),
            job.expected_vertex_count * FLOATS_PER_VERTEX
        ));
    }
    Ok(SkinningResult {
        stable_id: job.stable_id,
        source_model_id: job.source_model_id,
        generation: job.generation,
        upload_from: job.upload_from,
        expected_vertex_count: job.expected_vertex_count,
        packed,
        palette,
        gpu_skinning: job.gpu_skinning,
        feet,
    })
}
