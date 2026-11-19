use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use crate::common::{
    document::{read_document, write_document},
    error::{AppError, Result},
};

use super::super::model::{Coverage, Reading};
use super::model::{Document, ScopedMeasurement};

pub struct CostRepository {
    directory: PathBuf,
    access: Mutex<()>,
}

impl CostRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            access: Mutex::new(()),
        }
    }

    pub fn read(&self) -> Result<Document> {
        read_document(&self.directory.join("storage-costs.json"))
    }

    pub fn update(&self, change: impl FnOnce(&mut Document)) -> Result<()> {
        let _access = self
            .access
            .lock()
            .map_err(|_| AppError::File("Storage cost store is unavailable.".into()))?;
        let mut document = self.read()?;
        change(&mut document);
        write_document(
            &self.directory,
            &self.directory.join("storage-costs.json"),
            &document,
        )
    }
}

pub fn merge_measurements(document: &mut Document, incoming: Vec<ScopedMeasurement>) {
    let mut rows: HashMap<_, _> = document
        .measurements
        .drain(..)
        .map(|row| (row_key(&row), row))
        .collect();
    for mut row in incoming {
        let key = row_key(&row);
        if let Some(prior) = rows.remove(&key) {
            row.measurement.storage =
                prefer_reading(prior.measurement.storage, row.measurement.storage);
            row.measurement.traffic =
                prefer_reading(prior.measurement.traffic, row.measurement.traffic);
            if row.measurement.account_id.is_none() {
                row.measurement.account_id = prior.measurement.account_id;
            }
        }
        rows.insert(key, row);
    }
    document.measurements = rows.into_values().collect();
}

fn row_key(
    row: &ScopedMeasurement,
) -> (
    crate::administration::rgw_identities::model::RegistryScope,
    crate::administration::rgw_identities::model::IdentityTarget,
    chrono::NaiveDate,
) {
    (
        row.scope.clone(),
        row.measurement.subject.clone(),
        row.measurement.day,
    )
}

fn prefer_reading<T>(prior: Reading<T>, new: Reading<T>) -> Reading<T> {
    if (prior.coverage == Coverage::Complete && new.coverage != Coverage::Complete)
        || (prior.value.is_some() && new.value.is_none())
    {
        prior
    } else {
        new
    }
}
