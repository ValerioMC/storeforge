use uuid::Uuid;

use crate::{
    common::error::{AppError, Result},
    connection::shared_service::service::SharedService,
};

use super::{
    model::{HistoryFilter, HistoryQuery, HistoryScope, ServiceAuditEvent, ServiceAuditPage},
    repository::{HistoryReader, OperationsRepository},
};

const PAGE_SIZE: usize = 20;
const MAX_SOURCE_PAGES: usize = 5;

pub struct OperationsService<'a> {
    repository: OperationsRepository<'a>,
}

impl<'a> OperationsService<'a> {
    pub fn new(shared: &'a SharedService, id: Uuid) -> Self {
        Self {
            repository: OperationsRepository::new(shared, id),
        }
    }

    pub async fn history(
        &self,
        scope: HistoryScope,
        filter: HistoryFilter,
    ) -> Result<ServiceAuditPage> {
        let context_id = filter
            .context_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if scope == HistoryScope::Manager && context_id.is_none() {
            return Err(AppError::Invalid("Choose a Manager context.".into()));
        }
        let search = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let query = HistoryQuery {
            limit: PAGE_SIZE as u32,
            cursor: filter.cursor,
            context_id,
            search,
            status: if scope == HistoryScope::Manager {
                filter.status
            } else {
                None
            },
        };
        self.repository
            .history(scope, async |reader| {
                filtered_page(reader, query, &filter).await
            })
            .await
    }
}

async fn filtered_page(
    reader: HistoryReader<'_>,
    mut query: HistoryQuery<'_>,
    filter: &HistoryFilter,
) -> Result<ServiceAuditPage> {
    let actor = filter
        .actor
        .as_deref()
        .map(|actor| actor.trim().to_lowercase());
    let mut events = Vec::new();
    for _ in 0..MAX_SOURCE_PAGES {
        let source = reader.page(&query).await?;
        let mut rows = source.logs.into_iter().peekable();
        while let Some(event) = rows.next() {
            let event_id = event.id;
            if matches_filters(&event, actor.as_deref(), filter) {
                events.push(event);
            }
            if events.len() == PAGE_SIZE {
                let next_cursor =
                    (source.next_cursor.is_some() || rows.peek().is_some()).then_some(event_id);
                return Ok(ServiceAuditPage {
                    logs: events,
                    next_cursor,
                });
            }
        }
        if source.next_cursor.is_none() || source.next_cursor == query.cursor {
            return Ok(ServiceAuditPage {
                logs: events,
                next_cursor: None,
            });
        }
        query.cursor = source.next_cursor;
    }
    Ok(ServiceAuditPage {
        logs: events,
        next_cursor: query.cursor,
    })
}

fn matches_filters(event: &ServiceAuditEvent, actor: Option<&str>, filter: &HistoryFilter) -> bool {
    actor.is_none_or(|actor| event.user_email.to_lowercase().contains(actor))
        && filter.status.is_none_or(|status| status == event.status)
}
