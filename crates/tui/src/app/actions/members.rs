use super::super::*;

use crate::{
    app::{errors::login_message_for_error, format::member_role_rank},
    error::Result,
    text::{t, TextKey},
};
use api_types::membership::MemberUpsert;

impl App {
    pub(crate) async fn open_members(&mut self) -> Result<()> {
        self.state.section = Section::Settings;
        self.state.settings_tab = SettingsTab::Members;
        self.state.members.mode = MembersMode::List;
        self.reset_member_form();
        self.load_members().await
    }
    pub(crate) async fn load_members(&mut self) -> Result<()> {
        let vault_id = self.current_vault_id()?;
        let selected = self
            .state
            .members
            .items
            .get(self.state.members.selected)
            .map(|member| member.username.clone());

        let res = match self.state.members.scope {
            MembersScope::Vault => {
                self.client
                    .vault_members_list(vault_id.as_str())
                    .await
            }
            MembersScope::Flow => {
                self.ensure_member_flow_index();
                let Some((flow_id, _)) = self.current_member_flow() else {
                    self.state.members.items.clear();
                    self.state.members.selected = 0;
                    self.state.members.error =
                        Some(t(self.state.locale, TextKey::PromptNoShareableFlows).to_string());
                    return Ok(());
                };
                self.client
                    .flow_members_list(vault_id.as_str(),
                        flow_id,
                    )
                    .await
            }
        };

        match res {
            Ok(mut response) => {
                response.members.sort_by(|a, b| {
                    member_role_rank(a.role)
                        .cmp(&member_role_rank(b.role))
                        .then_with(|| a.username.cmp(&b.username))
                });
                self.state.members.items = response.members;
                if let Some(name) = selected {
                    if let Some(pos) = self
                        .state
                        .members
                        .items
                        .iter()
                        .position(|member| member.username == name)
                    {
                        self.state.members.selected = pos;
                    } else if self.state.members.selected >= self.state.members.items.len() {
                        self.state.members.selected =
                            self.state.members.items.len().saturating_sub(1);
                    }
                } else if self.state.members.selected >= self.state.members.items.len() {
                    self.state.members.selected = self.state.members.items.len().saturating_sub(1);
                }
                self.state.members.error = None;
                self.connection_ok(None);
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.members.error = Some(login_message_for_error(err, self.state.locale));
                self.connection_error(t(self.state.locale, TextKey::ErrorConnection));
            }
        }
        Ok(())
    }
    pub(crate) async fn set_members_scope(&mut self, scope: MembersScope) -> Result<()> {
        if self.state.members.scope == scope {
            return Ok(());
        }
        self.state.members.scope = scope;
        self.state.members.mode = MembersMode::List;
        self.reset_member_form();
        if scope == MembersScope::Flow {
            self.select_default_member_flow();
        }
        self.load_members().await
    }
    pub(crate) async fn submit_member_form(&mut self) -> Result<()> {
        // Validate the form
        if let Some(err) = self.state.members.form.validate_all() {
            self.state.members.error = Some(err);
            return Ok(());
        }

        let username = self.state.members.form.username.value().trim().to_string();
        if username.is_empty() {
            self.state.members.error =
                Some(t(self.state.locale, TextKey::PromptEnterUsername).to_string());
            return Ok(());
        }
        let vault_id = self.current_vault_id()?;
        let payload = MemberUpsert {
            username: username.clone(),
            role: self.state.members.form.role,
        };

        let res = match self.state.members.scope {
            MembersScope::Vault => {
                self.client
                    .vault_member_upsert(vault_id.as_str(),
                        payload,
                    )
                    .await
            }
            MembersScope::Flow => {
                let Some((flow_id, _)) = self.current_member_flow() else {
                    self.state.members.error =
                        Some(t(self.state.locale, TextKey::PromptNoShareableFlows).to_string());
                    return Ok(());
                };
                self.client
                    .flow_member_upsert(vault_id.as_str(),
                        flow_id,
                        payload,
                    )
                    .await
            }
        };

        match res {
            Ok(()) => {
                self.state.members.mode = MembersMode::List;
                self.reset_member_form();
                self.load_members().await?;
                self.select_member_by_username(username.as_str());
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.members.error = Some(login_message_for_error(err, self.state.locale));
            }
        }

        Ok(())
    }
    pub(crate) async fn remove_member(&mut self) -> Result<()> {
        let Some(member) = self.state.members.items.get(self.state.members.selected) else {
            self.state.members.error =
                Some(t(self.state.locale, TextKey::PromptNoMemberSelected).to_string());
            return Ok(());
        };
        let vault_id = self.current_vault_id()?;

        let res = match self.state.members.scope {
            MembersScope::Vault => {
                self.client
                    .vault_member_remove(vault_id.as_str(), member.username.as_str())
                    .await
            }
            MembersScope::Flow => {
                let Some((flow_id, _)) = self.current_member_flow() else {
                    self.state.members.error =
                        Some(t(self.state.locale, TextKey::PromptNoShareableFlows).to_string());
                    return Ok(());
                };
                self.client
                    .flow_member_remove(vault_id.as_str(),
                        flow_id,
                        member.username.as_str(),
                    )
                    .await
            }
        };

        match res {
            Ok(()) => {
                self.load_members().await?;
            }
            Err(err) => {
                if self.handle_auth_error(&err) {
                    return Ok(());
                }
                self.state.members.error = Some(login_message_for_error(err, self.state.locale));
            }
        }

        Ok(())
    }
}
