use api_types::{
    category::{
        CategoryList, CategoryMerge, CategoryMergeConflict, CategoryMergePreview, CategoryView,
    },
    membership::{MemberUpsert, MemberView, MembershipRole},
    vault::FlowView,
};
use reqwest::StatusCode;
use teloxide::prelude::*;
use uuid::Uuid;

use crate::{
    ConfigParameters,
    api::{ApiClient, ApiError},
    i18n::{self, TextKey},
    use_cases::shared,
};

pub(crate) async fn list_categories(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let response = match cfg
        .api
        .categories_list(
            user_id,
            &CategoryList {
                vault_id,
                include_archived: Some(true),
            },
        )
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let text = render_category_list(&response.categories);
    bot.send_message(chat_id, text).await?;
    Ok(())
}

pub(crate) async fn list_vault_members(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let response = match cfg.api.vault_members_list(user_id, &vault_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let text = render_members_list(
        i18n::t(locale, TextKey::MembersVaultTitle),
        &response.members,
    );
    bot.send_message(chat_id, text).await?;
    Ok(())
}

pub(crate) async fn add_vault_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    username: &str,
    role: MembershipRole,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let payload = MemberUpsert {
        username: username.to_string(),
        role,
    };
    match cfg
        .api
        .vault_member_upsert(user_id, &vault_id, &payload)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                i18n::format(
                    locale,
                    TextKey::MemberSaved,
                    &[("username", username), ("role", role_label(role))],
                ),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn remove_vault_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    username: &str,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    match cfg
        .api
        .vault_member_remove(user_id, &vault_id, username)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                i18n::format(locale, TextKey::MemberRemoved, &[("username", username)]),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn delete_vault(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    confirm: bool,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    if !confirm {
        bot.send_message(chat_id, vault_delete_help_text()).await?;
        return Ok(());
    }

    match cfg.api.vault_delete_main(user_id).await {
        Ok(()) => {
            bot.send_message(chat_id, i18n::t(locale, TextKey::VaultDeleted))
                .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn list_flow_members(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    let response = match cfg.api.flow_members_list(user_id, &vault_id, flow.id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let title = i18n::format(locale, TextKey::MembersFlowTitle, &[("flow", &flow.name)]);
    let text = render_members_list(&title, &response.members);
    bot.send_message(chat_id, text).await?;
    Ok(())
}

pub(crate) async fn add_flow_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
    username: &str,
    role: MembershipRole,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    let payload = MemberUpsert {
        username: username.to_string(),
        role,
    };
    match cfg
        .api
        .flow_member_upsert(user_id, &vault_id, flow.id, &payload)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                i18n::format(
                    locale,
                    TextKey::MemberSaved,
                    &[("username", username), ("role", role_label(role))],
                ),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn remove_flow_member(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    flow_name: &str,
    username: &str,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let flows = match resolve_accessible_flows(&cfg.api, user_id).await {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };
    let Some(flow) = find_flow_by_name(&flows, flow_name) else {
        bot.send_message(chat_id, flow_not_found_text(flow_name, &flows))
            .await?;
        return Ok(());
    };

    match cfg
        .api
        .flow_member_remove(user_id, &vault_id, flow.id, username)
        .await
    {
        Ok(()) => {
            bot.send_message(
                chat_id,
                i18n::format(locale, TextKey::MemberRemoved, &[("username", username)]),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn merge_category(
    bot: &Bot,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    confirm: bool,
    from: &str,
    into: &str,
) -> ResponseResult<()> {
    let locale = i18n::default_locale();
    let vault_id = match shared::resolve_main_vault_id(&cfg.api, user_id).await {
        Ok(id) => id,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let categories = match cfg
        .api
        .categories_list(
            user_id,
            &CategoryList {
                vault_id: vault_id.clone(),
                include_archived: Some(true),
            },
        )
        .await
    {
        Ok(resp) => resp.categories,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    let Some(from_category) = match_category_by_input(&categories, from) else {
        bot.send_message(
            chat_id,
            i18n::format(locale, TextKey::CategorySourceMissing, &[("name", from)]),
        )
        .await?;
        bot.send_message(chat_id, i18n::t(locale, TextKey::CategoryListHint))
            .await?;
        return Ok(());
    };
    let Some(into_category) = match_category_by_input(&categories, into) else {
        bot.send_message(
            chat_id,
            i18n::format(
                locale,
                TextKey::CategoryDestinationMissing,
                &[("name", into)],
            ),
        )
        .await?;
        bot.send_message(chat_id, i18n::t(locale, TextKey::CategoryListHint))
            .await?;
        return Ok(());
    };

    let preview = match cfg
        .api
        .categories_merge_preview(
            user_id,
            from_category.id,
            &CategoryMergePreview {
                vault_id: vault_id.clone(),
                into_category_id: into_category.id,
            },
        )
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
            return Ok(());
        }
    };

    if !preview.ok {
        let text = render_merge_conflicts(from_category, into_category, &preview);
        bot.send_message(chat_id, text).await?;
        return Ok(());
    }

    if !confirm {
        let text = i18n::format(
            locale,
            TextKey::MergeConfirmPrompt,
            &[("from", &from_category.name), ("into", &into_category.name)],
        );
        bot.send_message(chat_id, text).await?;
        return Ok(());
    }

    let merged = cfg
        .api
        .categories_merge(
            user_id,
            from_category.id,
            &CategoryMerge {
                vault_id,
                into_category_id: into_category.id,
            },
        )
        .await;
    match merged {
        Ok(_) => {
            bot.send_message(
                chat_id,
                i18n::format(
                    locale,
                    TextKey::MergeCompleted,
                    &[("from", &from_category.name), ("into", &into_category.name)],
                ),
            )
            .await?;
        }
        Err(err) => {
            bot.send_message(chat_id, shared::user_message_for_api_error(err))
                .await?;
        }
    }
    Ok(())
}

async fn resolve_accessible_flows(
    api: &ApiClient,
    telegram_user_id: u64,
) -> Result<Vec<FlowView>, ApiError> {
    match api.vault_snapshot_main(telegram_user_id).await {
        Ok(snapshot) => Ok(snapshot.flows),
        Err(ApiError::Server { status, .. }) if status == StatusCode::NOT_FOUND => {
            let response = api.flows_shared_main(telegram_user_id).await?;
            Ok(response.flows)
        }
        Err(err) => Err(err),
    }
}

fn render_category_list(categories: &[CategoryView]) -> String {
    let locale = i18n::default_locale();
    if categories.is_empty() {
        return i18n::t(locale, TextKey::CategoryListEmpty).to_string();
    }

    let mut lines = Vec::with_capacity(categories.len() + 2);
    lines.push(i18n::t(locale, TextKey::CategoryListHeader).to_string());
    for category in categories {
        let mut line = format!("- {}", category.name);
        if category.is_system {
            line.push_str(i18n::t(locale, TextKey::CategoryFlagSystem));
        }
        if category.archived {
            line.push_str(i18n::t(locale, TextKey::CategoryFlagArchived));
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn render_members_list(title: &str, members: &[MemberView]) -> String {
    let locale = i18n::default_locale();
    if members.is_empty() {
        return format!("{title}:\n- {}", i18n::t(locale, TextKey::MembersEmpty));
    }
    let mut lines = Vec::with_capacity(members.len() + 1);
    lines.push(format!("{title}:"));
    for member in members {
        lines.push(format!(
            "- {} ({})",
            member.username,
            role_label(member.role)
        ));
    }
    lines.join("\n")
}

fn role_label(role: MembershipRole) -> &'static str {
    let locale = i18n::default_locale();
    match role {
        MembershipRole::Owner => i18n::t(locale, TextKey::RoleOwner),
        MembershipRole::Editor => i18n::t(locale, TextKey::RoleEditor),
        MembershipRole::Viewer => i18n::t(locale, TextKey::RoleViewer),
    }
}

fn normalize_flow_label(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn find_flow_by_name<'a>(flows: &'a [FlowView], name: &str) -> Option<&'a FlowView> {
    let needle = normalize_flow_label(name);
    flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .find(|flow| normalize_flow_label(&flow.name) == needle)
}

fn flow_not_found_text(name: &str, flows: &[FlowView]) -> String {
    let locale = i18n::default_locale();
    let flows = flows
        .iter()
        .filter(|flow| !flow.archived && !flow.is_unallocated)
        .map(|flow| flow.name.as_str())
        .collect::<Vec<_>>();
    if flows.is_empty() {
        return i18n::format(locale, TextKey::FlowNotFoundNone, &[("name", name)]);
    }
    let mut lines = Vec::with_capacity(flows.len() + 2);
    lines.push(i18n::format(
        locale,
        TextKey::FlowNotFound,
        &[("name", name)],
    ));
    lines.push(i18n::t(locale, TextKey::FlowAvailableHeader).to_string());
    for flow in flows {
        lines.push(format!("- {flow}"));
    }
    lines.join("\n")
}

fn render_merge_conflicts(
    from: &CategoryView,
    into: &CategoryView,
    preview: &api_types::category::CategoryMergePreviewResponse,
) -> String {
    let locale = i18n::default_locale();
    let mut lines = Vec::with_capacity(preview.conflicts.len() + 2);
    lines.push(i18n::format(
        locale,
        TextKey::MergeConflictHeader,
        &[("from", &from.name), ("into", &into.name)],
    ));
    lines.push(i18n::t(locale, TextKey::MergeConflictListHeader).to_string());
    for conflict in &preview.conflicts {
        lines.push(format!("- {}", merge_conflict_label(conflict)));
    }
    lines.join("\n")
}

fn merge_conflict_label(conflict: &CategoryMergeConflict) -> String {
    let locale = i18n::default_locale();
    match conflict.kind.as_str() {
        "same_category" => i18n::t(locale, TextKey::MergeConflictSame).to_string(),
        "source_system" => i18n::format(
            locale,
            TextKey::MergeConflictSourceSystem,
            &[("name", &conflict.value)],
        ),
        "target_archived" => i18n::format(
            locale,
            TextKey::MergeConflictTargetArchived,
            &[("name", &conflict.value)],
        ),
        "alias_conflict" => i18n::format(
            locale,
            TextKey::MergeConflictAlias,
            &[("value", &conflict.value)],
        ),
        "name_conflict" => i18n::format(
            locale,
            TextKey::MergeConflictName,
            &[("value", &conflict.value)],
        ),
        _ => i18n::format(
            locale,
            TextKey::MergeConflictFallback,
            &[("kind", &conflict.kind), ("value", &conflict.value)],
        ),
    }
}

fn match_category_by_input<'a>(
    categories: &'a [CategoryView],
    input: &str,
) -> Option<&'a CategoryView> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(id) = Uuid::parse_str(trimmed) {
        return categories.iter().find(|category| category.id == id);
    }

    let needle = normalize_category_label(trimmed);
    categories
        .iter()
        .find(|category| normalize_category_label(&category.name) == needle)
}

fn normalize_category_label(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn vault_delete_help_text() -> &'static str {
    i18n::t(i18n::default_locale(), TextKey::VaultDeleteHelp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{category::CategoryView, membership::MemberView, vault::FlowView};

    fn category(id: u128, name: &str, archived: bool, is_system: bool) -> CategoryView {
        CategoryView {
            id: Uuid::from_u128(id),
            name: name.to_string(),
            archived,
            is_system,
        }
    }

    fn flow(id: u128, name: &str, archived: bool, is_unallocated: bool) -> FlowView {
        FlowView {
            id: Uuid::from_u128(id),
            name: name.to_string(),
            balance_minor: 0,
            archived,
            is_unallocated,
        }
    }

    #[test]
    fn render_category_list_empty() {
        let text = render_category_list(&[]);
        assert_eq!(
            text,
            "Nessuna categoria. Aggiungi una transazione con #categoria per iniziare."
        );
    }

    #[test]
    fn render_category_list_flags() {
        let categories = vec![category(1, "Food", true, true)];
        let text = render_category_list(&categories);
        assert!(text.contains("Food"));
        assert!(text.contains("[system]"));
        assert!(text.contains("[archived]"));
    }

    #[test]
    fn render_members_list_empty() {
        let text = render_members_list("Membri", &[]);
        assert_eq!(text, "Membri:\n- Nessun membro.");
    }

    #[test]
    fn render_members_list_values() {
        let members = vec![MemberView {
            username: "alice".to_string(),
            role: MembershipRole::Editor,
        }];
        let text = render_members_list("Membri", &members);
        assert!(text.contains("alice (editor)"));
    }

    #[test]
    fn find_flow_by_name_ignores_archived_and_unallocated() {
        let flows = vec![
            flow(1, "Main", false, true),
            flow(2, "Shared", true, false),
            flow(3, "My Flow", false, false),
        ];
        let found = find_flow_by_name(&flows, "  my   flow ");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, Uuid::from_u128(3));
    }

    #[test]
    fn flow_not_found_text_lists_available() {
        let flows = vec![flow(1, "One", false, false), flow(2, "Two", false, false)];
        let text = flow_not_found_text("Missing", &flows);
        assert!(text.contains("Flow \"Missing\" non trovato."));
        assert!(text.contains("Flow disponibili:"));
        assert!(text.contains("- One"));
        assert!(text.contains("- Two"));
    }

    #[test]
    fn merge_conflict_label_maps_known_kinds() {
        let conflict = CategoryMergeConflict {
            kind: "source_system".to_string(),
            value: "Sys".to_string(),
        };
        let label = merge_conflict_label(&conflict);
        assert_eq!(label, "La categoria \"Sys\" e' di sistema.");
    }

    #[test]
    fn match_category_by_input_matches_id_and_name() {
        let id = Uuid::from_u128(10);
        let categories = vec![CategoryView {
            id,
            name: "Food".to_string(),
            archived: false,
            is_system: false,
        }];

        let id_str = id.to_string();
        let by_id = match_category_by_input(&categories, &id_str);
        assert!(by_id.is_some());
        assert_eq!(by_id.unwrap().id, id);

        let by_name = match_category_by_input(&categories, "  food ");
        assert!(by_name.is_some());
        assert_eq!(by_name.unwrap().id, id);
    }
}
