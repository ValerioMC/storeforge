use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, error::CredentialsError, future},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::common::error::{AppError, Result};

use super::{
    super::{
        model::{CapabilityState, Connection, Provider, StoredConnection, Workspace},
        repository::ConnectionRepository,
    },
    model::{
        Activation, Authentication, CatalogTarget, Cloud, CloudSettings, CloudStatus,
        OpenStackProfile, Project, ProjectAccess, RegionalTarget, SessionView,
    },
    repository::{
        CloudRepository, KeystoneIdentity, KeystoneRepository, ProjectAuthority, keystone_url,
        normalized_endpoint, reconnect,
    },
};

const REVALIDATION_INTERVAL: Duration = Duration::from_secs(60);

pub struct OpenStackService {
    repository: CloudRepository,
    connections: Arc<ConnectionRepository>,
    clouds: Mutex<Vec<CloudState>>,
}

struct CloudState {
    cloud: Cloud,
    attempt: Uuid,
    session: Option<CloudSession>,
    reconnect_required: bool,
}

#[derive(Clone)]
struct Authority {
    identity: Arc<KeystoneIdentity>,
    client: Arc<KeystoneRepository>,
    revoked: Arc<AtomicBool>,
}

struct CloudSession {
    authority: Authority,
    projects: Vec<Project>,
    discoveries: HashMap<Uuid, Discovery>,
    leases: Vec<Arc<OpenStackLease>>,
}

#[derive(Clone)]
struct Discovery {
    project: Project,
    targets: Vec<CatalogTarget>,
    access_keys: Vec<String>,
}

pub(crate) struct OpenStackLease {
    pub profile: Connection,
    pub credentials: Credentials,
    authority: Authority,
    project_id: String,
    target: CatalogTarget,
    registration: Option<Registration>,
    connections: Arc<ConnectionRepository>,
    validation: tokio::sync::Mutex<Validation>,
}

struct Validation {
    expires_at: DateTime<Utc>,
    checked_at: Instant,
}

#[derive(PartialEq, Eq)]
struct Registration {
    id: Uuid,
    revision: Option<Uuid>,
}

pub(crate) struct ProjectCredentials {
    pub lease: Arc<OpenStackLease>,
    pub credentials: Credentials,
}

impl OpenStackService {
    pub fn open(connections: Arc<ConnectionRepository>) -> Result<Self> {
        let repository = CloudRepository::new(connections.directory());
        let clouds = repository
            .list()?
            .into_iter()
            .map(CloudState::new)
            .collect();
        Ok(Self {
            repository,
            connections,
            clouds: Mutex::new(clouds),
        })
    }

    pub fn list(&self) -> Result<Vec<CloudStatus>> {
        let mut clouds = self.clouds()?;
        Ok(clouds.iter_mut().map(CloudState::status).collect())
    }

    pub fn save(&self, id: Option<Uuid>, settings: CloudSettings) -> Result<Cloud> {
        let name = settings.name.trim().to_owned();
        if name.is_empty() {
            return Err(AppError::Invalid(
                "Enter a name for the OpenStack cloud.".into(),
            ));
        }
        let auth_url = keystone_url(&settings.auth_url)?
            .as_str()
            .trim_end_matches('/')
            .to_owned();
        let cloud = Cloud {
            id: id.unwrap_or_else(Uuid::new_v4),
            settings: CloudSettings {
                name,
                auth_url,
                default_domain: settings.default_domain.trim().into(),
                ..settings
            },
        };
        let mut clouds = self.clouds()?;
        if id.is_some() {
            find_cloud(&mut clouds, cloud.id)?;
        }
        let definitions = clouds
            .iter()
            .filter(|entry| entry.cloud.id != cloud.id)
            .map(|entry| &entry.cloud)
            .chain(std::iter::once(&cloud))
            .collect::<Vec<_>>();
        self.repository.write(&definitions)?;
        if let Some(existing) = clouds.iter_mut().find(|entry| entry.cloud.id == cloud.id) {
            existing.disconnect();
            *existing = CloudState::new(cloud.clone());
        } else {
            clouds.push(CloudState::new(cloud.clone()));
        }
        Ok(cloud)
    }

    pub fn disconnect(&self, id: Uuid, forget: bool) -> Result<()> {
        let mut clouds = self.clouds()?;
        if forget {
            find_cloud(&mut clouds, id)?;
            let definitions = clouds
                .iter()
                .filter(|entry| entry.cloud.id != id)
                .map(|entry| &entry.cloud)
                .collect::<Vec<_>>();
            self.repository.write(&definitions)?;
        }
        find_cloud(&mut clouds, id)?.disconnect();
        if forget {
            clouds.retain(|entry| entry.cloud.id != id);
        }
        Ok(())
    }

    pub async fn authenticate(
        &self,
        id: Uuid,
        authentication: Authentication,
    ) -> Result<SessionView> {
        let (attempt, cloud) = {
            let mut clouds = self.clouds()?;
            let entry = find_cloud(&mut clouds, id)?;
            entry.disconnect();
            (entry.attempt, entry.cloud.clone())
        };
        let client = Arc::new(KeystoneRepository::new(&cloud.settings.auth_url)?);
        let identity = client
            .authenticate(authentication, &cloud.settings.default_domain)
            .await?;
        let projects = client.projects(&identity).await?;
        let session = CloudSession {
            authority: Authority {
                identity: Arc::new(identity),
                client,
                revoked: Arc::default(),
            },
            projects,
            discoveries: HashMap::new(),
            leases: Vec::new(),
        };
        session.authority.ensure_active()?;
        let view = session.view();
        let mut clouds = self.clouds()?;
        let entry = find_cloud(&mut clouds, id)?;
        if entry.attempt != attempt {
            return Err(reconnect());
        }
        entry.session = Some(session);
        Ok(view)
    }

    pub async fn discover(&self, id: Uuid, project_id: String) -> Result<ProjectAccess> {
        let (cloud, authority, project) = {
            let mut clouds = self.clouds()?;
            let entry = find_cloud(&mut clouds, id)?;
            let cloud = entry.cloud.clone();
            let session = entry.live_session()?;
            let project = session
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Invalid("Choose an available OpenStack project.".into())
                })?;
            (cloud, session.authority.clone(), project)
        };
        let access = authority.project(&project.id).await?;
        let targets = self.connections.read(|entries| {
            Ok(access
                .targets
                .iter()
                .map(|target| {
                    let managed = registered_endpoint(entries, target).is_some();
                    RegionalTarget {
                        catalog: target.clone(),
                        managed,
                        allowed: managed || !cloud.settings.managed_endpoints_only,
                    }
                })
                .collect())
        })?;
        let access_keys: Vec<_> = access.keys.into_iter().map(|key| key.access).collect();
        let discovery_id = Uuid::new_v4();
        let mut clouds = self.clouds()?;
        let session = find_cloud(&mut clouds, id)?.current_session(&authority)?;
        session
            .discoveries
            .retain(|_, discovery| discovery.project.id != project_id);
        session.discoveries.insert(
            discovery_id,
            Discovery {
                project,
                targets: access.targets,
                access_keys: access_keys.clone(),
            },
        );
        Ok(ProjectAccess {
            id: discovery_id,
            targets,
            access_keys,
        })
    }

    pub async fn activate(&self, id: Uuid, selection: Activation) -> Result<Connection> {
        let (cloud, authority, discovery) = {
            let mut clouds = self.clouds()?;
            let entry = find_cloud(&mut clouds, id)?;
            let cloud = entry.cloud.clone();
            let session = entry.live_session()?;
            let discovery = session
                .discoveries
                .get(&selection.discovery_id)
                .cloned()
                .ok_or_else(|| AppError::Invalid("Load the OpenStack project again.".into()))?;
            (cloud, session.authority.clone(), discovery)
        };
        let target = discovery
            .targets
            .into_iter()
            .find(|target| target.id == selection.target_id)
            .ok_or_else(|| AppError::Invalid("Choose an available OpenStack S3 target.".into()))?;
        if !discovery.access_keys.contains(&selection.access_key) {
            return Err(AppError::Invalid(
                "Choose an existing EC2 credential for this project.".into(),
            ));
        }
        let access = authority.project(&discovery.project.id).await?;
        let credentials =
            activation_credentials(&authority, &target, &selection.access_key, &access)?;
        let mut clouds = self.clouds()?;
        let session = find_cloud(&mut clouds, id)?.current_session(&authority)?;
        if !session.discoveries.contains_key(&selection.discovery_id) {
            return Err(AppError::Invalid(
                "Load the OpenStack project again.".into(),
            ));
        }
        self.connections.read(|entries| {
            let registered = registered_endpoint(entries, &target);
            let registration = registered.map(Registration::from);
            if let Some(existing) = session.leases.iter().find(|lease| {
                lease.project_id == discovery.project.id
                    && lease.target == target
                    && lease.registration == registration
                    && lease.credentials.access_key_id() == credentials.access_key_id()
                    && lease.credentials.secret_access_key() == credentials.secret_access_key()
                    && lease
                        .profile
                        .settings
                        .openstack
                        .as_ref()
                        .is_some_and(|binding| binding.expires_at > Utc::now())
            }) {
                return Ok(existing.profile.clone());
            }
            let profile = project_profile(
                &cloud,
                &discovery.project,
                &target,
                registered,
                access.expires_at,
            )?;
            let lease = Arc::new(OpenStackLease {
                profile: profile.clone(),
                credentials,
                authority: authority.clone(),
                project_id: discovery.project.id,
                target,
                registration,
                connections: Arc::clone(&self.connections),
                validation: tokio::sync::Mutex::new(Validation {
                    expires_at: access.expires_at,
                    checked_at: Instant::now(),
                }),
            });
            session.leases.push(lease);
            Ok(profile)
        })
    }

    pub(crate) fn profiles(&self) -> Result<Vec<Connection>> {
        let mut clouds = self.clouds()?;
        let mut profiles = Vec::new();
        for cloud in clouds.iter_mut() {
            cloud.expire_session();
            if let Some(session) = &cloud.session
                && session
                    .leases
                    .iter()
                    .all(|lease| lease.ensure_active().is_ok())
            {
                profiles.extend(session.leases.iter().map(|lease| lease.profile.clone()));
            } else {
                cloud.expire_session();
            }
        }
        Ok(profiles)
    }

    pub(crate) fn lease(&self, id: Uuid) -> Result<Option<Arc<OpenStackLease>>> {
        let clouds = self.clouds()?;
        let lease = clouds
            .iter()
            .filter_map(|cloud| cloud.session.as_ref())
            .flat_map(|session| &session.leases)
            .find(|lease| lease.profile.id == id)
            .cloned();
        if let Some(lease) = &lease {
            lease.ensure_active()?;
        }
        Ok(lease)
    }

    fn clouds(&self) -> Result<MutexGuard<'_, Vec<CloudState>>> {
        self.clouds
            .lock()
            .map_err(|_| AppError::File("OpenStack connections are unavailable.".into()))
    }
}

impl CloudState {
    fn new(cloud: Cloud) -> Self {
        Self {
            cloud,
            attempt: Uuid::new_v4(),
            session: None,
            reconnect_required: false,
        }
    }

    fn disconnect(&mut self) {
        self.reconnect_required = false;
        self.attempt = Uuid::new_v4();
        if let Some(session) = self.session.take() {
            session.authority.revoke();
        }
    }

    fn expire_session(&mut self) {
        let expired = self
            .session
            .as_ref()
            .is_some_and(|session| session.authority.ensure_active().is_err());
        if expired {
            self.disconnect();
            self.reconnect_required = true;
        }
    }

    fn status(&mut self) -> CloudStatus {
        self.expire_session();
        CloudStatus {
            cloud: self.cloud.clone(),
            session: self.session.as_ref().map(CloudSession::view),
            reconnect_required: self.reconnect_required,
        }
    }

    fn live_session(&mut self) -> Result<&mut CloudSession> {
        self.expire_session();
        self.session.as_mut().ok_or_else(reconnect)
    }

    fn current_session(&mut self, authority: &Authority) -> Result<&mut CloudSession> {
        let session = self.live_session()?;
        if !Arc::ptr_eq(&session.authority.identity, &authority.identity) {
            return Err(reconnect());
        }
        Ok(session)
    }
}

impl CloudSession {
    fn view(&self) -> SessionView {
        SessionView {
            identity: self.authority.identity.name.clone(),
            expires_at: self.authority.identity.expires_at,
            projects: self.projects.clone(),
        }
    }
}

impl Authority {
    fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    fn ensure_active(&self) -> Result<()> {
        if self.revoked.load(Ordering::Acquire) || self.identity.expires_at <= Utc::now() {
            return Err(reconnect());
        }
        Ok(())
    }

    async fn project(&self, project: &str) -> Result<ProjectAuthority> {
        self.ensure_active()?;
        let result = self.client.project_authority(&self.identity, project).await;
        if matches!(result, Err(AppError::Reauthenticate(_))) {
            self.revoke();
        }
        self.ensure_active()?;
        result
    }

    fn changed(&self, message: &str) -> AppError {
        self.revoke();
        AppError::Reauthenticate(message.into())
    }
}

impl OpenStackLease {
    pub fn ensure_active(&self) -> Result<()> {
        self.authority.ensure_active()?;
        let binding = self
            .profile
            .settings
            .openstack
            .as_ref()
            .ok_or_else(reconnect)?;
        if binding.expires_at <= Utc::now() {
            self.authority.revoke();
            return Err(reconnect());
        }
        self.connections.read(|entries| {
            let registration = registered_endpoint(entries, &self.target).map(Registration::from);
            if registration != self.registration {
                return Err(self
                    .authority
                    .changed("The managed S3 endpoint changed. Reconnect to OpenStack."));
            }
            Ok(())
        })
    }

    pub async fn validate(&self) -> Result<()> {
        self.ensure_active()?;
        let mut validation = self.validation.lock().await;
        self.ensure_active()?;
        if validation.expires_at <= Utc::now() {
            self.authority.revoke();
            return Err(reconnect());
        }
        if validation.checked_at.elapsed() >= REVALIDATION_INTERVAL {
            let access = self.authority.project(&self.project_id).await?;
            let credentials = activation_credentials(
                &self.authority,
                &self.target,
                self.credentials.access_key_id(),
                &access,
            )?;
            if credentials.secret_access_key() != self.credentials.secret_access_key() {
                return Err(self
                    .authority
                    .changed("The EC2 credential changed. Reconnect to OpenStack."));
            }
            self.ensure_active()?;
            validation.expires_at = validation.expires_at.min(access.expires_at);
            validation.checked_at = Instant::now();
        }
        Ok(())
    }
}

impl std::fmt::Debug for ProjectCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectCredentials")
            .finish_non_exhaustive()
    }
}

impl ProvideCredentials for ProjectCredentials {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(async {
            self.lease
                .validate()
                .await
                .map_err(CredentialsError::provider_error)?;
            Ok(self.credentials.clone())
        })
    }
}

impl From<&Connection> for Registration {
    fn from(profile: &Connection) -> Self {
        Self {
            id: profile.id,
            revision: profile.revision,
        }
    }
}

fn find_cloud(clouds: &mut [CloudState], id: Uuid) -> Result<&mut CloudState> {
    clouds
        .iter_mut()
        .find(|entry| entry.cloud.id == id)
        .ok_or_else(|| AppError::Missing("This OpenStack cloud was removed.".into()))
}

fn registered_endpoint<'a>(
    entries: &'a [StoredConnection],
    target: &CatalogTarget,
) -> Option<&'a Connection> {
    entries
        .iter()
        .map(|entry| &entry.profile)
        .filter(|profile| {
            profile.settings.managed_iam.is_none()
                && profile.settings.managed_rgw.is_none()
                && normalized_endpoint(&profile.endpoint).as_deref()
                    == Some(target.endpoint.as_str())
        })
        .min_by_key(|profile| {
            (
                profile.region != target.region,
                !profile.settings.default_endpoint,
                profile.id,
            )
        })
}

fn project_profile(
    cloud: &Cloud,
    project: &Project,
    target: &CatalogTarget,
    registered: Option<&Connection>,
    expires_at: DateTime<Utc>,
) -> Result<Connection> {
    if registered.is_none() && cloud.settings.managed_endpoints_only {
        return Err(AppError::Invalid(
            "This cloud is limited to managed S3 endpoints.".into(),
        ));
    }
    let id = Uuid::new_v4();
    let mut settings = registered
        .map(|profile| profile.settings.clone())
        .unwrap_or_default();
    if !settings.browser_enabled && !settings.manager_enabled {
        return Err(AppError::Invalid(
            "This endpoint has no enabled Browser or Manager workspace.".into(),
        ));
    }
    settings.tenant.clear();
    settings.account.clone_from(&project.id);
    settings.realm.clear();
    settings.default_endpoint = false;
    if !matches!(
        settings.default_workspace,
        Some(Workspace::Browser | Workspace::Manager)
    ) {
        settings.default_workspace = None;
    }
    settings.openstack = Some(OpenStackProfile {
        cloud_id: cloud.id,
        project_name: project.name.clone(),
        expires_at,
    });
    Ok(Connection {
        id,
        name: format!(
            "{} / {} / {}",
            cloud.settings.name, project.name, target.region
        ),
        provider: registered
            .filter(|profile| profile.provider == Provider::Ceph)
            .map_or(Provider::Compatible, |profile| profile.provider),
        endpoint: target.endpoint.clone(),
        region: target.region.clone(),
        path_style: registered.is_none_or(|profile| profile.path_style),
        known_buckets: Vec::new(),
        endpoint_id: Some(registered.map_or(id, Connection::endpoint_id)),
        revision: Some(Uuid::new_v4()),
        settings,
        s3_capability: CapabilityState::Unknown,
        services: BTreeMap::new(),
    })
}

fn activation_credentials(
    authority: &Authority,
    target: &CatalogTarget,
    access_key: &str,
    access: &ProjectAuthority,
) -> Result<Credentials> {
    if !access.targets.contains(target) {
        return Err(authority.changed(
            "The OpenStack catalog target changed. Reconnect and select the region again.",
        ));
    }
    let key = access
        .keys
        .iter()
        .find(|key| key.access == access_key)
        .ok_or_else(|| authority.changed("The EC2 credential changed. Reconnect to OpenStack."))?;
    if access.expires_at <= Utc::now() {
        authority.revoke();
        return Err(reconnect());
    }
    Ok(Credentials::new(
        key.access.clone(),
        key.secret.clone(),
        None,
        Some(access.expires_at.into()),
        "openstack",
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/connection/openstack_context.rs"]
mod tests;
