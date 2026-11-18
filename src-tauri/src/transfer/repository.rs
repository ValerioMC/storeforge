use std::{path::PathBuf, sync::Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::model::{Job, JobStatus};

pub struct JobRepository {
    directory: PathBuf,
    jobs: Mutex<Vec<Job>>,
}

impl JobRepository {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let mut jobs: Vec<Job> = read_document(&directory.join("activity.json"))?;
        for job in &mut jobs {
            if job.status == JobStatus::Running {
                job.status = JobStatus::Interrupted;
                job.finished_at = Some(Utc::now());
                job.current = None;
            }
        }
        Ok(Self {
            directory,
            jobs: Mutex::new(jobs),
        })
    }

    pub fn list(&self) -> Result<Vec<Job>> {
        let jobs = self.jobs.lock().map_err(|_| unavailable())?;
        Ok(jobs.clone())
    }

    pub fn insert(&self, job: Job) -> Result<()> {
        let mut jobs = self.jobs.lock().map_err(|_| unavailable())?;
        let mut updated = jobs.clone();
        updated.insert(0, job);
        let mut finished = 0;
        updated.retain(|job| {
            if job.status == JobStatus::Running {
                return true;
            }
            finished += 1;
            finished <= 200
        });
        self.persist(&updated)?;
        *jobs = updated;
        Ok(())
    }

    pub fn update<T>(&self, id: Uuid, change: impl FnOnce(&mut Job) -> T) -> Result<T> {
        let mut jobs = self.jobs.lock().map_err(|_| unavailable())?;
        let job = jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| AppError::Missing("The job no longer exists.".into()))?;
        Ok(change(job))
    }

    pub fn checkpoint(&self) -> Result<()> {
        let jobs = self.jobs.lock().map_err(|_| unavailable())?;
        self.persist(&jobs)
    }

    fn persist(&self, jobs: &[Job]) -> Result<()> {
        write_document(&self.directory, &self.directory.join("activity.json"), jobs)
    }
}

fn unavailable() -> AppError {
    AppError::File("The activity history is unavailable.".into())
}
