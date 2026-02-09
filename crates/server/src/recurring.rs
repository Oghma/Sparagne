//! Recurring templates API endpoints.

use api_types::recurring::{
    PendingRecurringList, PendingRecurringListResponse, PendingRecurringView,
    RecurringExecute, RecurringExecuteResponse, RecurringKind, RecurringTemplateArchive,
    RecurringTemplateCreated, RecurringTemplateList, RecurringTemplateListResponse,
    RecurringTemplateNew, RecurringTemplateUpdate, RecurringTemplateView,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use crate::{ServerError, server::ServerState, user};

fn map_kind_to_engine(kind: RecurringKind) -> engine::TransactionKind {
    match kind {
        RecurringKind::Income => engine::TransactionKind::Income,
        RecurringKind::Expense => engine::TransactionKind::Expense,
    }
}

fn map_kind_from_engine(kind: engine::TransactionKind) -> RecurringKind {
    match kind {
        engine::TransactionKind::Income => RecurringKind::Income,
        _ => RecurringKind::Expense,
    }
}

fn map_frequency_to_engine(
    freq: api_types::recurring::RecurrenceFrequency,
) -> engine::RecurrenceFrequency {
    match freq {
        api_types::recurring::RecurrenceFrequency::Daily => engine::RecurrenceFrequency::Daily,
        api_types::recurring::RecurrenceFrequency::Weekly => engine::RecurrenceFrequency::Weekly,
        api_types::recurring::RecurrenceFrequency::Monthly => engine::RecurrenceFrequency::Monthly,
        api_types::recurring::RecurrenceFrequency::Yearly => engine::RecurrenceFrequency::Yearly,
    }
}

fn map_frequency_from_engine(
    freq: engine::RecurrenceFrequency,
) -> api_types::recurring::RecurrenceFrequency {
    match freq {
        engine::RecurrenceFrequency::Daily => api_types::recurring::RecurrenceFrequency::Daily,
        engine::RecurrenceFrequency::Weekly => api_types::recurring::RecurrenceFrequency::Weekly,
        engine::RecurrenceFrequency::Monthly => api_types::recurring::RecurrenceFrequency::Monthly,
        engine::RecurrenceFrequency::Yearly => api_types::recurring::RecurrenceFrequency::Yearly,
    }
}

fn template_to_view(t: &engine::RecurringTemplate) -> RecurringTemplateView {
    RecurringTemplateView {
        id: t.id,
        kind: map_kind_from_engine(t.kind),
        amount_minor: t.amount_minor,
        wallet_id: t.wallet_id,
        flow_id: t.flow_id,
        category_id: t.category_id,
        note: t.note.clone(),
        frequency: map_frequency_from_engine(t.frequency),
        day_of_period: t.day_of_period,
        start_date: t.start_date.format("%Y-%m-%d").to_string(),
        end_date: t.end_date.map(|d| d.format("%Y-%m-%d").to_string()),
        enabled: t.enabled,
        last_executed_date: t.last_executed_date.map(|d| d.format("%Y-%m-%d").to_string()),
    }
}

pub async fn create(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<RecurringTemplateNew>,
) -> Result<(StatusCode, Json<RecurringTemplateCreated>), ServerError> {
    let start_date = NaiveDate::parse_from_str(&payload.start_date, "%Y-%m-%d")
        .map_err(|e| ServerError::Generic(format!("invalid start_date: {e}")))?;
    let end_date = payload
        .end_date
        .as_deref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|e| ServerError::Generic(format!("invalid end_date: {e}")))
        })
        .transpose()?;

    let mut cmd = engine::CreateRecurringCmd::new(
        &payload.vault_id,
        &user.username,
        map_kind_to_engine(payload.kind),
        payload.amount_minor,
        map_frequency_to_engine(payload.frequency),
        payload.day_of_period,
        start_date,
    );
    cmd.wallet_id = payload.wallet_id;
    cmd.flow_id = payload.flow_id;
    cmd.category_id = payload.category_id;
    cmd.category = payload.category;
    cmd.note = payload.note;
    cmd.end_date = end_date;

    let id = state.engine.create_recurring(cmd).await?;
    Ok((StatusCode::CREATED, Json(RecurringTemplateCreated { id })))
}

pub async fn list(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<RecurringTemplateList>,
) -> Result<Json<RecurringTemplateListResponse>, ServerError> {
    let templates = state
        .engine
        .list_recurring(&payload.vault_id, &user.username, payload.include_archived)
        .await?;

    let views: Vec<RecurringTemplateView> = templates.iter().map(template_to_view).collect();
    Ok(Json(RecurringTemplateListResponse { templates: views }))
}

pub async fn get(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RecurringTemplateList>,
) -> Result<Json<RecurringTemplateView>, ServerError> {
    let template = state
        .engine
        .get_recurring(&payload.vault_id, id, &user.username)
        .await?;
    Ok(Json(template_to_view(&template)))
}

pub async fn update(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RecurringTemplateUpdate>,
) -> Result<StatusCode, ServerError> {
    let mut cmd = engine::UpdateRecurringCmd::new(&payload.vault_id, id, &user.username);

    if let Some(v) = payload.amount_minor {
        cmd = cmd.amount_minor(v);
    }
    if let Some(v) = payload.wallet_id {
        cmd = cmd.wallet_id(v);
    }
    if let Some(v) = payload.flow_id {
        cmd = cmd.flow_id(v);
    }
    if let Some(v) = payload.category_id {
        cmd = cmd.category_id(v);
    }
    if let Some(v) = payload.category {
        cmd = cmd.category(v);
    }
    if let Some(v) = payload.note {
        cmd = cmd.note(v);
    }
    if let Some(v) = payload.frequency {
        cmd = cmd.frequency(map_frequency_to_engine(v));
    }
    if let Some(v) = payload.day_of_period {
        cmd = cmd.day_of_period(v);
    }
    if let Some(v) = payload.end_date {
        let parsed = v
            .as_deref()
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map_err(|e| ServerError::Generic(format!("invalid end_date: {e}")))
            })
            .transpose()?;
        cmd = cmd.end_date(parsed);
    }
    if let Some(v) = payload.enabled {
        cmd = cmd.enabled(v);
    }

    state.engine.update_recurring(cmd).await?;
    Ok(StatusCode::OK)
}

pub async fn archive(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RecurringTemplateArchive>,
) -> Result<StatusCode, ServerError> {
    state
        .engine
        .archive_recurring(&payload.vault_id, id, &user.username)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pending(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Json(payload): Json<PendingRecurringList>,
) -> Result<Json<PendingRecurringListResponse>, ServerError> {
    let today = Utc::now().date_naive();
    let pending_list = state
        .engine
        .list_pending_recurring(&payload.vault_id, &user.username, today)
        .await?;

    let views: Vec<PendingRecurringView> = pending_list
        .iter()
        .map(|p| PendingRecurringView {
            template: template_to_view(&p.template),
            period_date: p.period_date.format("%Y-%m-%d").to_string(),
        })
        .collect();

    Ok(Json(PendingRecurringListResponse { pending: views }))
}

pub async fn execute(
    Extension(user): Extension<user::Model>,
    State(state): State<ServerState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RecurringExecute>,
) -> Result<(StatusCode, Json<RecurringExecuteResponse>), ServerError> {
    let today = Utc::now().date_naive();
    let tx_id = state
        .engine
        .execute_recurring(&payload.vault_id, id, &user.username, today)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(RecurringExecuteResponse {
            transaction_id: tx_id,
        }),
    ))
}
