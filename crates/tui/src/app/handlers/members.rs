use super::super::*;

use crate::error::Result;
use api_types::membership::MembershipRole;

impl App {
    pub(crate) fn members_select_next(&mut self) {
        let len = self.state.members.items.len();
        if len == 0 {
            return;
        }
        self.state.members.selected = (self.state.members.selected + 1).min(len - 1);
    }

    pub(crate) fn members_select_prev(&mut self) {
        if self.state.members.items.is_empty() {
            return;
        }
        self.state.members.selected = self.state.members.selected.saturating_sub(1);
    }

    pub(crate) fn members_flow_next(&mut self) {
        let len = self.member_flow_options().len();
        if len == 0 {
            self.state.members.flow_index = 0;
            return;
        }
        self.state.members.flow_index = (self.state.members.flow_index + 1).min(len - 1);
    }

    pub(crate) fn members_flow_prev(&mut self) {
        if self.member_flow_options().is_empty() {
            self.state.members.flow_index = 0;
            return;
        }
        self.state.members.flow_index = self.state.members.flow_index.saturating_sub(1);
    }
    pub(crate) fn member_flow_options(&self) -> Vec<(uuid::Uuid, String)> {
        let Some(snapshot) = self.state.snapshot.as_ref() else {
            return Vec::new();
        };
        snapshot
            .flows
            .iter()
            .filter(|flow| !flow.archived && !flow.is_unallocated)
            .map(|flow| (flow.id, flow.name.clone()))
            .collect()
    }

    pub(crate) fn current_member_flow(&self) -> Option<(uuid::Uuid, String)> {
        let flows = self.member_flow_options();
        flows.get(self.state.members.flow_index).cloned()
    }

    pub(crate) fn start_member_create(&mut self) {
        self.state.members.mode = MembersMode::Form;
        self.state.members.form = MemberFormState::default();
    }

    pub(crate) fn start_member_edit(&mut self) {
        let Some(member) = self.state.members.items.get(self.state.members.selected) else {
            self.state.members.error = Some("Nessun membro selezionato.".to_string());
            return;
        };
        self.state.members.mode = MembersMode::Form;
        self.state
            .members
            .form
            .username
            .set_value(member.username.clone());
        self.state.members.form.role = member.role;
        self.state.members.form.focus = MemberFormField::Role;
        self.state.members.form.editing = true;
        self.state.members.form.update_focus();
        self.state.members.error = None;
    }
    pub(crate) fn cycle_member_role(&mut self, forward: bool) {
        self.state.members.form.role = match (self.state.members.form.role, forward) {
            (MembershipRole::Owner, true) => MembershipRole::Editor,
            (MembershipRole::Editor, true) => MembershipRole::Viewer,
            (MembershipRole::Viewer, true) => MembershipRole::Owner,
            (MembershipRole::Owner, false) => MembershipRole::Viewer,
            (MembershipRole::Editor, false) => MembershipRole::Owner,
            (MembershipRole::Viewer, false) => MembershipRole::Editor,
        };
    }

    pub(crate) fn select_member_by_username(&mut self, username: &str) {
        if let Some(pos) = self
            .state
            .members
            .items
            .iter()
            .position(|member| member.username == username)
        {
            self.state.members.selected = pos;
        }
    }

    pub(crate) fn ensure_member_flow_index(&mut self) {
        let len = self.member_flow_options().len();
        if len == 0 {
            self.state.members.flow_index = 0;
            return;
        }
        if self.state.members.flow_index >= len {
            self.state.members.flow_index = len - 1;
        }
    }

    pub(crate) fn select_default_member_flow(&mut self) {
        let flows = self.member_flow_options();
        if flows.is_empty() {
            self.state.members.flow_index = 0;
            return;
        }
        if let Some(last_flow_id) = self.state.last_flow_id
            && let Some(pos) = flows
                .iter()
                .position(|(flow_id, _)| *flow_id == last_flow_id)
        {
            self.state.members.flow_index = pos;
            return;
        }
        self.state.members.flow_index = 0;
    }

    pub(crate) async fn handle_members_input(&mut self, ch: char) -> Result<bool> {
        if !(self.state.section == Section::Settings
            && self.state.settings_tab == SettingsTab::Members)
        {
            return Ok(false);
        }
        if self.state.members.mode == MembersMode::Form {
            return Ok(false);
        }

        match ch {
            'a' | 'A' => {
                if self.state.members.mode == MembersMode::List {
                    self.start_member_create();
                }
                return Ok(true);
            }
            'e' | 'E' => {
                if self.state.members.mode == MembersMode::List {
                    self.start_member_edit();
                }
                return Ok(true);
            }
            'x' | 'X' => {
                if self.state.members.mode == MembersMode::List {
                    self.remove_member().await?;
                }
                return Ok(true);
            }
            '[' => {
                if self.state.members.mode == MembersMode::List
                    && self.state.members.scope == MembersScope::Flow
                {
                    self.members_flow_prev();
                    self.load_members().await?;
                    return Ok(true);
                }
            }
            ']' => {
                if self.state.members.mode == MembersMode::List
                    && self.state.members.scope == MembersScope::Flow
                {
                    self.members_flow_next();
                    self.load_members().await?;
                    return Ok(true);
                }
            }
            'v' | 'V' => {
                if self.state.members.mode == MembersMode::List {
                    self.set_members_scope(MembersScope::Vault).await?;
                    return Ok(true);
                }
            }
            'f' | 'F' => {
                if self.state.members.mode == MembersMode::List {
                    self.set_members_scope(MembersScope::Flow).await?;
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }
}
