use super::super::*;

use api_types::{
    flow::FlowSharedList,
    transaction::{TransactionGet, TransactionKind, TransactionList},
    vault::{FlowView, Vault, VaultSnapshot},
};
use chrono::Duration as ChronoDuration;

use crate::{
    app::{
        errors::login_message_for_error,
        ordering::push_recent_id,
        resolve::extract_wallet_flow,
    },
    client::ClientError,
    error::{AppError, Result},
};

impl App {
    pub(crate) fn update_recent_categories_from_items(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut categories = Vec::new();
        for tx in &self.state.transactions.items {
            if let Some(category) = tx.category.as_ref() {
                let key = category.to_lowercase();
                if seen.insert(key) {
                    categories.push(category.clone());
                }
            }
            if categories.len() >= 5 {
                break;
            }
        }
        self.state.transactions.recent_categories = categories;
    }

    pub(crate) async fn refresh_recent_targets(&mut self) -> Result<()> {
        const RECENTS_LIMIT: usize = 5;
        const RECENTS_FETCH_LIMIT: u64 = 50;
        const RECENTS_WINDOW_DAYS: i64 = 90;

        let vault_id = match self.current_vault_id() {
            Ok(id) => id,
            Err(_) => return Ok(()),
        };
        let to = self.now_in_timezone();
        let from = to - ChronoDuration::days(RECENTS_WINDOW_DAYS);

        let payload = TransactionList {
            vault_id: vault_id.clone(),
            flow_id: self.state.transactions.scope_flow_id,
            wallet_id: self.state.transactions.scope_wallet_id,
            limit: Some(RECENTS_FETCH_LIMIT),
            cursor: None,
            from: Some(from),
            to: Some(to),
            kinds: Some(vec![
                TransactionKind::Income,
                TransactionKind::Expense,
                TransactionKind::Refund,
            ]),
            include_voided: Some(false),
            include_transfers: Some(false),
        };

        let res = self
            .client
            .transactions_list(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                payload,
            )
            .await;

        let Ok(list) = res else {
            return Ok(());
        };

        let mut categories = Vec::new();
        let mut seen_categories = std::collections::HashSet::new();
        for tx in &list.transactions {
            if let Some(category) = tx.category.as_ref() {
                let key = category.to_lowercase();
                if seen_categories.insert(key) {
                    categories.push(category.clone());
                }
            }
            if categories.len() >= RECENTS_LIMIT {
                break;
            }
        }
        if !categories.is_empty() {
            self.state.transactions.recent_categories = categories;
        }

        let mut recent_wallets = Vec::new();
        let mut recent_flows = Vec::new();
        for tx in &list.transactions {
            if recent_wallets.len() >= RECENTS_LIMIT && recent_flows.len() >= RECENTS_LIMIT {
                break;
            }
            let detail = self
                .client
                .transaction_detail(
                    self.state.login.username.as_str(),
                    self.state.login.password.as_str(),
                    TransactionGet {
                        vault_id: vault_id.clone(),
                        id: tx.id,
                    },
                )
                .await;
            let Ok(detail) = detail else {
                continue;
            };
            let (wallet_id, flow_id) = extract_wallet_flow(&detail);
            if let Some(wallet_id) = wallet_id {
                push_recent_id(&mut recent_wallets, wallet_id, RECENTS_LIMIT);
            }
            if let Some(flow_id) = flow_id {
                push_recent_id(&mut recent_flows, flow_id, RECENTS_LIMIT);
            }
        }

        if !recent_wallets.is_empty() {
            self.state.transactions.recent_wallet_ids = recent_wallets;
        }
        if !recent_flows.is_empty() {
            self.state.transactions.recent_flow_ids = recent_flows;
        }

        Ok(())
    }
    pub(crate) async fn refresh_snapshot(&mut self) -> Result<()> {
        let vault_payload = self.current_vault_payload();
        let res = self
            .client
            .vault_snapshot(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                &vault_payload,
            )
            .await;

        match res {
            Ok(snapshot) => {
                self.apply_snapshot(snapshot);
                self.refresh_wallets_search().await?;
                self.refresh_flows_search().await?;
                self.connection_ok(None);
                self.state.overlays.error = None;
            }
            Err(ClientError::NotFound(_)) => {
                if let Err(err) = self.refresh_shared_flows_snapshot().await {
                    self.state.wallets.error = Some(err.to_string());
                    self.state.flows.error = Some(err.to_string());
                    self.state.stats.error = Some(err.to_string());
                    self.connection_error("Errore connessione");
                    self.state.overlays.error = Some(ErrorDialogState::connection(
                        "Connection Error",
                        "Unable to connect to server.",
                        Some(err.to_string()),
                        ErrorAction::RetrySnapshot,
                    ));
                } else {
                    self.connection_ok(None);
                    self.state.overlays.error = None;
                }
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                let message = login_message_for_error(err, self.state.locale);
                let detail = Some(message.clone());
                self.state.wallets.error = Some(message.clone());
                self.state.flows.error = Some(message.clone());
                self.state.stats.error = Some(message);
                self.connection_error("Errore connessione");
                self.state.overlays.error = Some(ErrorDialogState::connection(
                    "Connection Error",
                    "Unable to connect to server.",
                    detail,
                    ErrorAction::RetrySnapshot,
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: VaultSnapshot) {
        self.state.snapshot = Some(snapshot);
        self.ensure_last_flow();
        self.normalize_defaults();
        self.ensure_flow_scope_for_shared();
        if self.state.members.scope == MembersScope::Flow {
            self.ensure_member_flow_index();
        }
    }

    pub(crate) fn ensure_flow_scope_for_shared(&mut self) {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return;
        };
        if snapshot.wallets.is_empty() {
            self.state.transactions.scope_wallet_id = None;
            self.state.transactions.scope_flow_id = snapshot.flows.first().map(|flow| flow.id);
        }
    }

    pub(crate) fn build_shared_snapshot(&self, flows: Vec<FlowView>) -> Result<VaultSnapshot> {
        let vault = self
            .state
            .vault
            .as_ref()
            .ok_or_else(|| AppError::Terminal("vault metadata missing".to_string()))?;
        let vault_id = vault
            .id
            .as_ref()
            .ok_or_else(|| AppError::Terminal("vault id missing".to_string()))?;
        let name = vault.name.clone().unwrap_or_else(|| vault_id.to_string());
        let currency = vault.currency.unwrap_or(api_types::Currency::Eur);
        let unallocated_flow_id = flows
            .iter()
            .find_map(|flow| flow.is_unallocated.then_some(flow.id))
            .or_else(|| flows.first().map(|flow| flow.id))
            .unwrap_or_else(uuid::Uuid::nil);

        Ok(VaultSnapshot {
            id: vault_id.clone(),
            name,
            currency,
            owner: vault.owner.clone(),
            wallets: Vec::new(),
            flows,
            unallocated_flow_id,
        })
    }

    pub(crate) async fn refresh_shared_flows_snapshot(&mut self) -> Result<()> {
        let vault_id = self
            .state
            .vault
            .as_ref()
            .and_then(|vault| vault.id.as_deref())
            .ok_or_else(|| AppError::Terminal("vault id missing".to_string()))?;

        let res = self
            .client
            .flows_shared_list(
                self.state.login.username.as_str(),
                self.state.login.password.as_str(),
                FlowSharedList {
                    vault_id: vault_id.to_string(),
                    include_archived: Some(true),
                },
            )
            .await;

        match res {
            Ok(response) => {
                let snapshot = self.build_shared_snapshot(response.flows)?;
                self.apply_snapshot(snapshot);
                self.refresh_wallets_search().await?;
                self.refresh_flows_search().await?;
                Ok(())
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                Err(AppError::Terminal(login_message_for_error(err, self.state.locale)))
            }
        }
    }

    pub(crate) fn ensure_last_flow(&mut self) {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return;
        };
        let last_valid = self
            .state
            .last_flow_id
            .and_then(|last| snapshot.flows.iter().find(|flow| flow.id == last))
            .map(|flow| flow.id);
        self.state.last_flow_id = last_valid.or(Some(snapshot.unallocated_flow_id));
    }

    pub(crate) fn current_vault_payload(&self) -> Vault {
        if let Some(vault) = self.state.vault.as_ref() {
            return Vault {
                id: vault.id.clone(),
                name: vault.name.clone(),
                currency: None,
                owner: None,
            };
        }
        self.vault_payload_from_config()
    }

    pub(crate) fn current_vault_id(&self) -> Result<String> {
        self.state
            .vault
            .as_ref()
            .and_then(|vault| vault.id.clone())
            .ok_or_else(|| AppError::Terminal("missing vault id".to_string()))
    }

    pub(crate) fn vault_payload_from_config(&self) -> Vault {
        let raw = self.config.vault.trim();
        if raw.is_empty() {
            return Vault {
                id: None,
                name: None,
                currency: None,
                owner: None,
            };
        }

        if let Some(stripped) = raw.strip_prefix("id:") {
            let id = stripped.trim();
            return Vault {
                id: (!id.is_empty()).then_some(id.to_string()),
                name: None,
                currency: None,
                owner: None,
            };
        }

        if uuid::Uuid::parse_str(raw).is_ok() {
            return Vault {
                id: Some(raw.to_string()),
                name: None,
                currency: None,
                owner: None,
            };
        }

        Vault {
            id: None,
            name: Some(raw.to_string()),
            currency: None,
            owner: None,
        }
    }
}
