use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use url::{Host, Url};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::common::error::{AppError, Result};

use super::{
    model::{
        InteractiveAction, ServiceDraft, ServicePrincipal, ServiceProfile, ServiceSession,
        ServiceStatus, SharedContext, SharedWorkspace, WorkspaceAccess, WorkspaceAvailability,
    },
    repository::{ProfileRepository, SharedServiceRepository},
};

pub struct SharedService {
    profiles: Mutex<BTreeMap<Uuid, SavedService>>,
    repository: ProfileRepository,
    remote: SharedServiceRepository,
}

struct SavedService {
    profile: ServiceProfile,
    revision: Uuid,
    session: Option<Arc<SessionLease>>,
}

struct SessionLease {
    token: Zeroizing<String>,
    view: ServiceSession,
}

#[derive(Deserialize)]
struct ExpiryClaims {
    typ: String,
    exp: i64,
}

#[derive(Deserialize)]
struct RuntimeSurfaces {
    admin: bool,
    #[serde(default)]
    manager: bool,
    #[serde(default)]
    browser: bool,
    #[serde(default)]
    portal: bool,
}

impl SharedService {
    pub fn open(directory: PathBuf) -> Result<Self> {
        let repository = ProfileRepository::new(directory);
        let profiles = repository
            .list()?
            .into_iter()
            .map(|profile| {
                (
                    profile.id,
                    SavedService {
                        profile,
                        revision: Uuid::new_v4(),
                        session: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            profiles: Mutex::new(profiles),
            repository,
            remote: SharedServiceRepository::new()?,
        })
    }

    pub fn list(&self) -> Result<Vec<ServiceStatus>> {
        let mut profiles = self.profiles()?;
        let mut statuses: Vec<_> = profiles
            .values_mut()
            .map(|entry| ServiceStatus {
                profile: entry.profile.clone(),
                session: entry.current_session().map(|session| session.view.clone()),
            })
            .collect();
        statuses.sort_by(|first, second| {
            first
                .profile
                .settings
                .name
                .cmp(&second.profile.settings.name)
        });
        Ok(statuses)
    }

    pub fn save(&self, id: Option<Uuid>, draft: ServiceDraft) -> Result<ServiceProfile> {
        let name = draft.name.trim().to_owned();
        if name.is_empty() {
            return Err(AppError::Invalid("Enter a service name.".into()));
        }
        let origin = service_origin(&draft.origin)?
            .origin()
            .ascii_serialization();
        let mut profiles = self.profiles()?;
        if let Some(id) = id {
            get_profile(&mut profiles, id)?;
        }
        let id = id.unwrap_or_else(Uuid::new_v4);
        let profile = ServiceProfile {
            id,
            settings: ServiceDraft { name, origin },
        };
        self.repository.write(
            &profiles
                .values()
                .filter(|entry| entry.profile.id != id)
                .map(|entry| &entry.profile)
                .chain(std::iter::once(&profile))
                .collect::<Vec<_>>(),
        )?;
        profiles.insert(
            id,
            SavedService {
                profile: profile.clone(),
                revision: Uuid::new_v4(),
                session: None,
            },
        );
        Ok(profile)
    }

    pub fn disconnect(&self, id: Uuid, forget: bool) -> Result<()> {
        let mut profiles = self.profiles()?;
        get_profile(&mut profiles, id)?;
        if forget {
            self.repository.write(
                &profiles
                    .values()
                    .filter(|entry| entry.profile.id != id)
                    .map(|entry| &entry.profile)
                    .collect::<Vec<_>>(),
            )?;
            profiles.remove(&id);
        } else {
            get_profile(&mut profiles, id)?.clear_session();
        }
        Ok(())
    }

    pub async fn connect(&self, id: Uuid, token: String) -> Result<ServiceSession> {
        let token = Zeroizing::new(token);
        let expires_at = token_expiry(token.trim())?;
        let (revision, origin) = {
            let mut profiles = self.profiles()?;
            let entry = get_profile(&mut profiles, id)?;
            entry.clear_session();
            (
                entry.revision,
                service_origin(&entry.profile.settings.origin)?,
            )
        };
        let principal: ServicePrincipal = self
            .remote
            .get(
                &origin,
                "/api/users/me",
                &[] as &[(&str, &str)],
                Some(token.trim()),
            )
            .await?;
        if !principal.active {
            return Err(AppError::AuthorizationDenied(
                "This shared service account is inactive.".into(),
            ));
        }
        let access = self
            .workspace_access(&origin, &principal, token.trim())
            .await?;
        let mut profiles = self.profiles()?;
        let entry = current_profile(&mut profiles, id, revision)?;
        if expires_at <= Utc::now() {
            return Err(reauthorize());
        }
        let view = ServiceSession {
            principal,
            access,
            expires_at,
        };
        entry.session = Some(Arc::new(SessionLease {
            token,
            view: view.clone(),
        }));
        Ok(view)
    }

    async fn workspace_access(
        &self,
        origin: &Url,
        principal: &ServicePrincipal,
        token: &str,
    ) -> Result<WorkspaceAccess> {
        let catalog = self
            .remote
            .get(
                origin,
                "/api/me/workspace-access",
                &[] as &[(&str, &str)],
                Some(token),
            )
            .await;
        let Err(AppError::Missing(_)) = &catalog else {
            return catalog;
        };
        let surfaces = self.runtime_surfaces(origin).await?;
        if !surfaces.admin || surfaces.manager || surfaces.browser || surfaces.portal {
            return catalog;
        }
        let available = principal.role.is_admin();
        Ok(WorkspaceAccess {
            admin: WorkspaceAvailability {
                available,
                context_count: u64::from(available),
            },
            ..WorkspaceAccess::default()
        })
    }

    async fn runtime_surfaces(&self, origin: &Url) -> Result<RuntimeSurfaces> {
        self.remote
            .get(
                origin,
                "/api/settings/runtime-surfaces",
                &[] as &[(&str, &str)],
                None,
            )
            .await
    }

    pub async fn contexts(
        &self,
        id: Uuid,
        workspace: SharedWorkspace,
    ) -> Result<Vec<SharedContext>> {
        self.scoped(
            id,
            |session| workspace.available(&session.access),
            async |remote, origin, token| {
                remote
                    .get(
                        origin,
                        "/api/me/execution-contexts",
                        &[("workspace", workspace)],
                        Some(token),
                    )
                    .await
            },
        )
        .await
    }

    pub(crate) async fn scoped<T>(
        &self,
        id: Uuid,
        available: impl FnOnce(&ServiceSession) -> bool,
        request: impl AsyncFnOnce(&SharedServiceRepository, &Url, &str) -> Result<T>,
    ) -> Result<T> {
        let (revision, origin, session) = {
            let mut profiles = self.profiles()?;
            let entry = get_profile(&mut profiles, id)?;
            let session = entry.current_session().ok_or_else(reauthorize)?;
            if !available(&session.view) {
                return Err(AppError::AuthorizationDenied(
                    "This workspace is unavailable for the shared service connection.".into(),
                ));
            }
            (
                entry.revision,
                service_origin(&entry.profile.settings.origin)?,
                session,
            )
        };
        let result = request(&self.remote, &origin, session.token.trim()).await;
        let mut profiles = self.profiles()?;
        let entry = current_profile(&mut profiles, id, revision)?;
        if matches!(result, Err(AppError::Reauthenticate(_))) {
            entry.clear_session();
        } else if entry.current_session().is_none() {
            return Err(reauthorize());
        }
        result
    }

    pub async fn interactive_url(&self, id: Uuid, action: InteractiveAction) -> Result<Url> {
        let (revision, origin) = {
            let mut profiles = self.profiles()?;
            let entry = get_profile(&mut profiles, id)?;
            (
                entry.revision,
                service_origin(&entry.profile.settings.origin)?,
            )
        };
        if action.admin_only() {
            let surfaces = self.runtime_surfaces(&origin).await?;
            if !surfaces.admin {
                return Err(AppError::AuthorizationDenied(
                    "The service does not expose the Admin workspace.".into(),
                ));
            }
        }
        current_profile(&mut *self.profiles()?, id, revision)?;
        action.url(&origin)
    }

    fn profiles(&self) -> Result<MutexGuard<'_, BTreeMap<Uuid, SavedService>>> {
        self.profiles
            .lock()
            .map_err(|_| AppError::File("Shared service profiles are unavailable.".into()))
    }
}

impl SavedService {
    fn clear_session(&mut self) {
        self.session = None;
        self.revision = Uuid::new_v4();
    }

    fn current_session(&mut self) -> Option<Arc<SessionLease>> {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.view.expires_at <= Utc::now())
        {
            self.clear_session();
        }
        self.session.clone()
    }
}

fn get_profile(profiles: &mut BTreeMap<Uuid, SavedService>, id: Uuid) -> Result<&mut SavedService> {
    profiles
        .get_mut(&id)
        .ok_or_else(|| AppError::Missing("The saved shared service no longer exists.".into()))
}

fn current_profile(
    profiles: &mut BTreeMap<Uuid, SavedService>,
    id: Uuid,
    revision: Uuid,
) -> Result<&mut SavedService> {
    let entry = get_profile(profiles, id)?;
    if entry.revision != revision {
        return Err(reauthorize());
    }
    Ok(entry)
}

fn reauthorize() -> AppError {
    AppError::Reauthenticate("Authorize the shared service again.".into())
}

fn service_origin(value: &str) -> Result<Url> {
    let invalid = || {
        AppError::Invalid(
            "Enter an HTTPS service origin, such as https://storage.example.org.".into(),
        )
    };
    let url = Url::parse(value.trim()).map_err(|_| invalid())?;
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !matches!(url.scheme(), "https") && !(url.scheme() == "http" && loopback)
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url)
}

fn token_expiry(token: &str) -> Result<DateTime<Utc>> {
    let mut parts = token.split('.');
    let invalid = || AppError::Invalid("Enter a scoped API token from the shared service.".into());
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    if header.is_empty() || signature.is_empty() || token.len() > 16_384 {
        return Err(invalid());
    }
    let payload = Zeroizing::new(URL_SAFE_NO_PAD.decode(payload).map_err(|_| invalid())?);
    let claims: ExpiryClaims = serde_json::from_slice(&payload).map_err(|_| invalid())?;
    let expires_at = DateTime::from_timestamp(claims.exp, 0).ok_or_else(invalid)?;
    if claims.typ != "api_access" {
        return Err(invalid());
    }
    if expires_at <= Utc::now() {
        return Err(reauthorize());
    }
    Ok(expires_at)
}
