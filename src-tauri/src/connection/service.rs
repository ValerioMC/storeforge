use aws_sdk_s3::{
    Client,
    config::{
        BehaviorVersion, Credentials, Region, RequestChecksumCalculation,
        ResponseChecksumValidation,
    },
};
use url::Url;
use uuid::Uuid;

use crate::common::error::{AppError, Result};

use super::{
    model::{Connection, ConnectionDraft, Provider, Secret, StoredConnection},
    repository::ConnectionRepository,
};

pub struct Session {
    pub profile: Connection,
    pub credentials: Credentials,
    pub s3: Client,
}

pub struct ConnectionService {
    repository: ConnectionRepository,
}

impl ConnectionService {
    pub fn new(repository: ConnectionRepository) -> Self {
        Self { repository }
    }

    pub fn list(&self) -> Result<Vec<Connection>> {
        self.repository
            .read(|entries| Ok(entries.iter().map(|entry| entry.profile.clone()).collect()))
    }

    pub fn save(&self, draft: ConnectionDraft) -> Result<Connection> {
        let profile = validate_profile(&draft)?;
        let new_secret = match draft.credentials {
            Some(secret) => {
                validate_secret(&secret)?;
                let id = Uuid::new_v4();
                credential_entry(id)?.set_password(&serde_json::to_string(&secret)?)?;
                Some(id)
            }
            None => None,
        };
        let update = self
            .repository
            .update(|entries| update_profile(entries, &profile, new_secret));
        match update {
            Ok(replaced) => {
                remove_secret(replaced);
                Ok(profile)
            }
            Err(error) => {
                remove_secret(new_secret);
                Err(error)
            }
        }
    }

    pub fn remove(&self, id: Uuid) -> Result<()> {
        let secret_id = self.repository.update(|entries| {
            let position = entries
                .iter()
                .position(|entry| entry.profile.id == id)
                .ok_or_else(missing_profile)?;
            Ok(entries.remove(position).secret_id)
        })?;
        remove_secret(Some(secret_id));
        Ok(())
    }

    pub fn session(&self, id: Uuid) -> Result<Session> {
        let (profile, secret) = self.stored_credentials(id)?;
        Ok(open_session(profile, secret))
    }

    pub fn draft_session(&self, mut draft: ConnectionDraft) -> Result<Session> {
        let profile = validate_profile(&draft)?;
        let secret = match draft.credentials.take() {
            Some(secret) => secret,
            None => self.stored_credentials(profile.id)?.1,
        };
        validate_secret(&secret)?;
        Ok(open_session(profile, secret))
    }

    fn stored_credentials(&self, id: Uuid) -> Result<(Connection, Secret)> {
        self.repository.read(|entries| {
            let entry = entries
                .iter()
                .find(|entry| entry.profile.id == id)
                .ok_or_else(missing_profile)?;
            Ok((entry.profile.clone(), read_secret(entry.secret_id)?))
        })
    }
}

fn validate_profile(draft: &ConnectionDraft) -> Result<Connection> {
    let name = draft.name.trim();
    let region = draft.region.trim();
    if name.is_empty() || region.is_empty() {
        return Err(AppError::Invalid(
            "Enter a connection name and region.".into(),
        ));
    }
    let endpoint = validate_endpoint(draft.endpoint.trim(), draft.provider)?;
    Ok(Connection {
        id: draft.id.unwrap_or_else(Uuid::new_v4),
        name: name.into(),
        provider: draft.provider,
        endpoint,
        region: region.into(),
        path_style: draft.path_style,
        known_buckets: draft
            .known_buckets
            .iter()
            .map(|bucket| bucket.trim().to_owned())
            .filter(|bucket| !bucket.is_empty())
            .collect(),
    })
}

fn validate_endpoint(input: &str, provider: Provider) -> Result<String> {
    if input.is_empty() && provider == Provider::Aws {
        return Ok(String::new());
    }
    let invalid = || {
        AppError::Invalid("Enter an HTTP or HTTPS endpoint URL without credentials, query parameters, or a fragment.".into())
    };
    let url = Url::parse(input).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url.as_str().trim_end_matches('/').into())
}

fn validate_secret(secret: &Secret) -> Result<()> {
    if secret.access_key.trim().is_empty() || secret.secret_key.trim().is_empty() {
        return Err(AppError::Invalid(
            "Enter an access key and secret key.".into(),
        ));
    }
    Ok(())
}

fn update_profile(
    entries: &mut Vec<StoredConnection>,
    profile: &Connection,
    secret: Option<Uuid>,
) -> Result<Option<Uuid>> {
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.profile.id == profile.id)
    {
        entry.profile = profile.clone();
        return Ok(secret.map(|id| std::mem::replace(&mut entry.secret_id, id)));
    }
    let secret_id =
        secret.ok_or_else(|| AppError::Invalid("A new connection requires credentials.".into()))?;
    entries.push(StoredConnection {
        profile: profile.clone(),
        secret_id,
    });
    Ok(None)
}

fn missing_profile() -> AppError {
    AppError::Missing("The connection was removed. Select another connection.".into())
}

fn credential_entry(id: Uuid) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        "dev.vmcforge.storeforge",
        &id.to_string(),
    )?)
}

fn read_secret(id: Uuid) -> Result<Secret> {
    Ok(serde_json::from_str(
        &credential_entry(id)?.get_password()?,
    )?)
}

fn remove_secret(id: Option<Uuid>) {
    if let Some(id) = id {
        let result = keyring::Entry::new("dev.vmcforge.storeforge", &id.to_string())
            .and_then(|entry| entry.delete_credential());
        if let Err(error) = result
            && !matches!(error, keyring::Error::NoEntry)
        {
            tracing::warn!(%error, "Could not remove unused credentials");
        }
    }
}

pub fn open_session(profile: Connection, secret: Secret) -> Session {
    let credentials = Credentials::new(
        secret.access_key,
        secret.secret_key,
        secret.session_token,
        None,
        "storeforge",
    );
    let mut builder = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(profile.region.clone()))
        .credentials_provider(credentials.clone())
        .force_path_style(profile.path_style)
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(ResponseChecksumValidation::WhenRequired);
    if !profile.endpoint.is_empty() {
        builder = builder.endpoint_url(&profile.endpoint);
    }
    Session {
        profile,
        credentials,
        s3: Client::from_conf(builder.build()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/connection/service.rs"]
mod tests;
