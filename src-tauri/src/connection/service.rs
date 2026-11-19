use std::{collections::BTreeMap, time::Duration};

use aws_sdk_s3::{
    Client,
    config::{
        BehaviorVersion, Credentials, Region, RequestChecksumCalculation,
        ResponseChecksumValidation,
    },
};
use aws_smithy_http_client::{
    Builder,
    tls::{Provider as TlsProvider, TlsContext, TrustStore, rustls_provider::CryptoMode},
};
use rustls_pki_types::{CertificateDer, pem::PemObject};
use url::Url;
use uuid::Uuid;

use crate::common::error::{AppError, Result};

use super::{
    model::{
        CapabilityState, Connection, ConnectionCheck, ConnectionDraft, ExecutionContext,
        IdentityKind, ManagedIamOwner, Provider, Secret, ServiceDraft, ServiceIdentity,
        StoredConnection, Workspace,
    },
    repository::{ConnectionRepository, CredentialRepository, KeyringRepository},
};

mod probe;

pub struct Session {
    pub context: ExecutionContext,
    pub profile: Connection,
    pub credentials: Credentials,
    pub s3: Client,
}

pub struct ConnectionService {
    repository: ConnectionRepository,
    credentials: Box<dyn CredentialRepository>,
}

struct PreparedConnection {
    baseline: Option<Uuid>,
    stored: StoredConnection,
    secrets: BTreeMap<IdentityKind, Secret>,
}

impl ConnectionService {
    pub fn new(repository: ConnectionRepository) -> Self {
        Self {
            repository,
            credentials: Box::new(KeyringRepository),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_credentials(
        repository: ConnectionRepository,
        credentials: impl CredentialRepository + 'static,
    ) -> Self {
        Self {
            repository,
            credentials: Box::new(credentials),
        }
    }

    pub fn list(&self) -> Result<Vec<Connection>> {
        self.repository
            .read(|entries| Ok(entries.iter().map(|entry| entry.profile.clone()).collect()))
    }

    pub(crate) fn with_profile<T>(
        &self,
        id: Uuid,
        read: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<T> {
        self.repository
            .read(|entries| read(&profile_by_id(entries, id)?.profile))
    }

    pub async fn save(&self, draft: ConnectionDraft) -> Result<Connection> {
        let mut prepared = self.prepare(&draft)?;
        self.check_prepared(&mut prepared, &draft).await?;
        let mut written = Vec::new();
        let outcome = self.persist(&mut prepared, draft.id.is_some(), &mut written);
        match outcome {
            Ok(replaced) => {
                self.remove_secrets(replaced);
                Ok(prepared.stored.profile)
            }
            Err(error) => {
                self.remove_secrets(written);
                Err(error)
            }
        }
    }

    pub async fn check(&self, draft: ConnectionDraft) -> Result<ConnectionCheck> {
        let mut prepared = self.prepare(&draft)?;
        self.check_prepared(&mut prepared, &draft).await
    }

    pub fn remove(&self, id: Uuid) -> Result<()> {
        let removed = self.repository.update(|entries| {
            let position = entries
                .iter()
                .position(|entry| entry.profile.id == id)
                .ok_or_else(missing_profile)?;
            ensure_unmanaged(&entries[position].profile)?;
            Ok(entries.remove(position))
        })?;
        self.remove_secrets(
            std::iter::once(removed.secret_id).chain(removed.service_secret_ids.into_values()),
        );
        Ok(())
    }

    pub(crate) fn save_managed_iam(
        &self,
        parent: &Connection,
        name: String,
        owner: ManagedIamOwner,
        workspaces: (bool, bool),
        secret: Secret,
    ) -> Result<Connection> {
        validate_secret(&secret)?;
        let id = owner.provisioning_id;
        let mut profile = parent.clone();
        profile.id = id;
        profile.endpoint_id = Some(parent.endpoint_id());
        profile.revision = Some(Uuid::new_v4());
        profile.name = name;
        profile.s3_capability = CapabilityState::Unknown;
        profile.known_buckets.clear();
        profile.services.clear();
        profile.settings.default_endpoint = false;
        (
            profile.settings.browser_enabled,
            profile.settings.manager_enabled,
        ) = workspaces;
        profile.settings.default_workspace = None;
        profile.settings.managed_iam = Some(owner);
        self.credentials.write(id, &secret)?;
        match self.repository.update(|entries| {
            entries.push(StoredConnection {
                profile: profile.clone(),
                secret_id: id,
                service_secret_ids: BTreeMap::new(),
            });
            Ok(profile)
        }) {
            Ok(profile) => Ok(profile),
            Err(error) => {
                self.credentials.remove(id);
                Err(error)
            }
        }
    }

    pub(crate) fn remove_managed_iam(&self, id: Uuid) -> Result<()> {
        self.repository.update(|entries| {
            if let Some(position) = entries.iter().position(|entry| {
                entry.profile.id == id && entry.profile.settings.managed_iam.is_some()
            }) {
                entries.remove(position);
            }
            Ok(())
        })?;
        self.credentials.remove(id);
        Ok(())
    }

    pub fn session(&self, id: Uuid) -> Result<Session> {
        self.session_for(id, Workspace::Browser, IdentityKind::S3)
    }

    pub fn session_for(
        &self,
        id: Uuid,
        workspace: Workspace,
        identity: IdentityKind,
    ) -> Result<Session> {
        let (profile, context, secret) = self.repository.read(|entries| {
            let entry = profile_by_id(entries, id)?;
            let context = resolve_context(&entry.profile, workspace, identity)?;
            let reference = secret_reference(entry, identity)?;
            Ok((
                service_profile(&entry.profile, identity),
                context,
                self.credentials.read(reference)?,
            ))
        })?;
        open_session(profile, context, secret)
    }

    fn prepare(&self, draft: &ConnectionDraft) -> Result<PreparedConnection> {
        self.repository.read(|entries| {
            let existing = draft.id.map(|id| profile_by_id(entries, id)).transpose()?;
            if let Some(entry) = existing {
                ensure_unmanaged(&entry.profile)?;
            }
            let profile = validate_profile(draft, existing.map(|entry| &entry.profile), entries)?;
            let mut stored = existing.cloned().unwrap_or_else(|| StoredConnection {
                profile: profile.clone(),
                secret_id: Uuid::new_v4(),
                service_secret_ids: BTreeMap::new(),
            });
            stored.profile = profile;
            let mut secrets = BTreeMap::new();
            if let Some(secret) = &draft.credentials {
                validate_secret(secret)?;
                secrets.insert(IdentityKind::S3, secret.clone());
            } else if existing.is_none() {
                return Err(AppError::Invalid(
                    "A new connection requires S3 credentials.".into(),
                ));
            }
            for (&identity, change) in &draft.reconcile {
                reconcile_service(&mut stored, &mut secrets, identity, change)?;
            }
            Ok(PreparedConnection {
                baseline: existing.and_then(|entry| entry.profile.revision),
                stored,
                secrets,
            })
        })
    }

    async fn check_prepared(
        &self,
        prepared: &mut PreparedConnection,
        draft: &ConnectionDraft,
    ) -> Result<ConnectionCheck> {
        let mut bucket_count = None;
        if draft.validate_s3 {
            let (state, count) = self.check_identity(prepared, IdentityKind::S3).await?;
            prepared.stored.profile.s3_capability = state;
            bucket_count = count;
        }
        for &identity in draft.reconcile.keys() {
            if prepared.stored.profile.services[&identity].enabled {
                let (state, _) = self.check_identity(prepared, identity).await?;
                if let Some(service) = prepared.stored.profile.services.get_mut(&identity) {
                    service.capability = state;
                }
            }
        }
        let capabilities = [
            IdentityKind::S3,
            IdentityKind::AdminOps,
            IdentityKind::Runtime,
            IdentityKind::Supervision,
            IdentityKind::CephAdmin,
        ]
        .into_iter()
        .map(|identity| (identity, prepared.stored.profile.capability(identity)))
        .collect();
        Ok(ConnectionCheck {
            bucket_count,
            capabilities,
        })
    }

    async fn check_identity(
        &self,
        prepared: &PreparedConnection,
        identity: IdentityKind,
    ) -> Result<(CapabilityState, Option<usize>)> {
        let secret = match prepared.secrets.get(&identity) {
            Some(secret) => secret.clone(),
            None => self
                .credentials
                .read(secret_reference(&prepared.stored, identity)?)?,
        };
        let profile = &prepared.stored.profile;
        let context = describe_context(profile, Workspace::Admin, identity)?;
        let session = open_session(service_profile(profile, identity), context, secret)?;
        Ok(
            tokio::time::timeout(Duration::from_secs(10), probe::check(&session, identity))
                .await
                .unwrap_or((CapabilityState::Unknown, None)),
        )
    }

    fn persist(
        &self,
        prepared: &mut PreparedConnection,
        editing: bool,
        written: &mut Vec<Uuid>,
    ) -> Result<Vec<Uuid>> {
        self.repository.update(|entries| {
            let current = if editing {
                let current = entries
                    .iter_mut()
                    .find(|entry| entry.profile.id == prepared.stored.profile.id)
                    .ok_or_else(missing_profile)?;
                if current.profile.revision != prepared.baseline {
                    return Err(AppError::Invalid(
                        "This profile changed while it was being checked. Reopen it and try again."
                            .into(),
                    ));
                }
                Some(current)
            } else {
                None
            };
            let mut replaced = Vec::new();
            for (&identity, secret) in &prepared.secrets {
                let reference = Uuid::new_v4();
                written.push(reference);
                self.credentials.write(reference, secret)?;
                let old = if identity == IdentityKind::S3 {
                    let old = std::mem::replace(&mut prepared.stored.secret_id, reference);
                    editing.then_some(old)
                } else {
                    prepared
                        .stored
                        .service_secret_ids
                        .insert(identity, reference)
                };
                replaced.extend(old);
            }
            if let Some(current) = current {
                *current = prepared.stored.clone();
            } else {
                entries.push(prepared.stored.clone());
            }
            if prepared.stored.profile.settings.default_endpoint {
                for other in entries
                    .iter_mut()
                    .filter(|entry| entry.profile.id != prepared.stored.profile.id)
                {
                    other.profile.settings.default_endpoint = false;
                }
            }
            Ok(replaced)
        })
    }

    fn remove_secrets(&self, references: impl IntoIterator<Item = Uuid>) {
        for reference in references {
            self.credentials.remove(reference);
        }
    }
}

fn ensure_unmanaged(profile: &Connection) -> Result<()> {
    if profile.settings.managed_iam.is_some() {
        return Err(AppError::Invalid(
            "Manage this connection through IAM personal access.".into(),
        ));
    }
    Ok(())
}

fn profile_by_id(entries: &[StoredConnection], id: Uuid) -> Result<&StoredConnection> {
    entries
        .iter()
        .find(|entry| entry.profile.id == id)
        .ok_or_else(missing_profile)
}

fn resolve_context(
    profile: &Connection,
    workspace: Workspace,
    identity: IdentityKind,
) -> Result<ExecutionContext> {
    identity_available(profile, identity)?;
    describe_context(profile, workspace, identity)
}

pub fn describe_context(
    profile: &Connection,
    workspace: Workspace,
    identity: IdentityKind,
) -> Result<ExecutionContext> {
    let principal_id = profile
        .principal_id(identity)
        .ok_or_else(|| identity_error(identity, CapabilityState::Unavailable))?;
    Ok(ExecutionContext {
        connection_id: profile.id,
        endpoint_id: profile.endpoint_id(),
        principal_id,
        identity,
        tenant: profile.settings.tenant.clone(),
        account: profile.settings.account.clone(),
        region: profile.region.clone(),
        workspace,
    })
}

fn identity_available(profile: &Connection, identity: IdentityKind) -> Result<()> {
    match profile.capability(identity) {
        CapabilityState::Detected => Ok(()),
        CapabilityState::Unknown if identity == IdentityKind::S3 => Ok(()),
        state => Err(identity_error(identity, state)),
    }
}

fn identity_error(identity: IdentityKind, state: CapabilityState) -> AppError {
    let reason = match state {
        CapabilityState::Unknown => "has not been verified",
        CapabilityState::Unavailable => "is unavailable",
        CapabilityState::Denied => "was denied by the endpoint",
        CapabilityState::Detected => "has no saved credentials",
    };
    AppError::Invalid(format!(
        "{} {reason} for this connection.",
        identity.label()
    ))
}

fn validate_profile(
    draft: &ConnectionDraft,
    current: Option<&Connection>,
    entries: &[StoredConnection],
) -> Result<Connection> {
    let name = draft.name.trim();
    let region = draft.region.trim();
    if name.is_empty() || region.is_empty() {
        return Err(AppError::Invalid(
            "Enter a connection name and region.".into(),
        ));
    }
    let endpoint = validate_endpoint(draft.endpoint.trim(), draft.provider)?;
    let mut settings = draft
        .settings
        .clone()
        .or_else(|| current.map(|profile| profile.settings.clone()))
        .unwrap_or_default();
    settings.managed_iam = None;
    settings.tenant = settings.tenant.trim().into();
    settings.account = settings.account.trim().into();
    settings.realm = settings.realm.trim().into();
    validate_ca(&settings.ca_certificate)?;
    if settings.coordinates.is_some_and(|point| {
        !(-90.0..=90.0).contains(&point.latitude) || !(-180.0..=180.0).contains(&point.longitude)
    }) {
        return Err(AppError::Invalid(
            "Enter latitude from −90 to 90 and longitude from −180 to 180.".into(),
        ));
    }
    let id = draft.id.unwrap_or_else(Uuid::new_v4);
    let same_endpoint = |profile: &Connection| {
        profile.provider == draft.provider
            && profile.endpoint == endpoint
            && profile.region == region
    };
    let endpoint_id = current
        .filter(|profile| same_endpoint(profile))
        .map(Connection::endpoint_id)
        .or_else(|| {
            entries
                .iter()
                .map(|entry| &entry.profile)
                .find(|profile| same_endpoint(profile))
                .map(Connection::endpoint_id)
        })
        .unwrap_or_else(|| {
            if current.is_some() {
                Uuid::new_v4()
            } else {
                id
            }
        });
    let known_buckets: Vec<String> = draft
        .known_buckets
        .iter()
        .map(|bucket| bucket.trim().to_owned())
        .filter(|bucket| !bucket.is_empty())
        .collect();
    let same_scope = current.is_some_and(|profile| {
        same_endpoint(profile)
            && draft.path_style == profile.path_style
            && known_buckets == profile.known_buckets
            && settings.tenant == profile.settings.tenant
            && settings.account == profile.settings.account
            && settings.realm == profile.settings.realm
            && settings.ca_certificate == profile.settings.ca_certificate
    });
    let mut services = current
        .map(|profile| profile.services.clone())
        .unwrap_or_default();
    if !same_scope {
        for service in services.values_mut() {
            service.capability = CapabilityState::Unknown;
        }
    }
    let s3_capability = current
        .filter(|_| same_scope && draft.credentials.is_none())
        .map_or(CapabilityState::Unknown, |profile| profile.s3_capability);
    Ok(Connection {
        id,
        name: name.into(),
        provider: draft.provider,
        endpoint,
        region: region.into(),
        path_style: draft.path_style,
        known_buckets,
        endpoint_id: Some(endpoint_id),
        revision: Some(Uuid::new_v4()),
        settings,
        s3_capability,
        services,
    })
}

fn reconcile_service(
    stored: &mut StoredConnection,
    secrets: &mut BTreeMap<IdentityKind, Secret>,
    identity: IdentityKind,
    change: &ServiceDraft,
) -> Result<()> {
    if identity == IdentityKind::S3
        || (change.enabled && identity.requires_ceph() && stored.profile.provider != Provider::Ceph)
    {
        return Err(identity_error(identity, CapabilityState::Unavailable));
    }
    let previous = stored.profile.services.get(&identity);
    if previous.is_some_and(|service| service.managed_externally) {
        return Err(AppError::Invalid(format!(
            "{} settings are externally managed.",
            identity.label()
        )));
    }
    let endpoint = if change.endpoint.trim().is_empty() {
        String::new()
    } else {
        validate_endpoint(change.endpoint.trim(), stored.profile.provider)?
    };
    if let Some(secret) = change.credentials.as_ref() {
        validate_secret(secret)?;
        secrets.insert(identity, secret.clone());
    } else if change.enabled && !stored.service_secret_ids.contains_key(&identity) {
        return Err(AppError::Invalid(format!(
            "Enter {} credentials.",
            identity.label()
        )));
    }
    stored.profile.services.insert(
        identity,
        ServiceIdentity {
            principal_id: previous.map_or_else(Uuid::new_v4, |service| service.principal_id),
            endpoint,
            enabled: change.enabled,
            managed_externally: false,
            capability: CapabilityState::Unknown,
        },
    );
    Ok(())
}

fn service_profile(profile: &Connection, identity: IdentityKind) -> Connection {
    let mut profile = profile.clone();
    if let Some(service) = profile
        .services
        .get(&identity)
        .filter(|service| !service.endpoint.is_empty())
    {
        profile.endpoint.clone_from(&service.endpoint);
    }
    profile
}

fn secret_reference(entry: &StoredConnection, identity: IdentityKind) -> Result<Uuid> {
    if identity == IdentityKind::S3 {
        return Ok(entry.secret_id);
    }
    entry
        .service_secret_ids
        .get(&identity)
        .copied()
        .ok_or_else(|| identity_error(identity, CapabilityState::Detected))
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

fn validate_ca(pem: &str) -> Result<()> {
    if pem.trim().is_empty() {
        return Ok(());
    }
    let invalid = || AppError::Invalid("Enter a PEM-encoded CA certificate or bundle.".into());
    if pem.len() > 262_144 || pem.contains("PRIVATE KEY") {
        return Err(invalid());
    }
    let mut roots = rustls::RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(pem.as_bytes()) {
        roots
            .add(certificate.map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    }
    if roots.is_empty() {
        return Err(invalid());
    }
    Ok(())
}

fn validate_secret(secret: &Secret) -> Result<()> {
    if secret.access_key.trim().is_empty() || secret.secret_key.trim().is_empty() {
        return Err(AppError::Invalid(
            "Enter an access key and secret key.".into(),
        ));
    }
    Ok(())
}

fn missing_profile() -> AppError {
    AppError::Missing("The connection was removed. Select another connection.".into())
}

pub fn open_session(
    profile: Connection,
    context: ExecutionContext,
    secret: Secret,
) -> Result<Session> {
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
    if !profile.settings.ca_certificate.trim().is_empty() {
        validate_ca(&profile.settings.ca_certificate)?;
        let trust =
            TrustStore::default().with_pem_certificate(profile.settings.ca_certificate.as_bytes());
        let tls = TlsContext::builder()
            .with_trust_store(trust)
            .build()
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        let client = Builder::new()
            .tls_provider(TlsProvider::Rustls(CryptoMode::AwsLc))
            .tls_context(tls)
            .build_https();
        builder = builder.http_client(client);
    }
    Ok(Session {
        profile,
        context,
        credentials,
        s3: Client::from_conf(builder.build()),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/connection/service.rs"]
mod tests;
