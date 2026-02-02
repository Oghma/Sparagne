use super::super::*;

impl App {
    pub(crate) fn wallets_select_next(&mut self) {
        let len = wallets_visible_indices(&self.state).len();
        if len == 0 {
            return;
        }
        self.state.wallets.selected = (self.state.wallets.selected + 1).min(len - 1);
    }

    pub(crate) fn wallets_select_prev(&mut self) {
        if wallets_visible_indices(&self.state).is_empty() {
            return;
        }
        self.state.wallets.selected = self.state.wallets.selected.saturating_sub(1);
    }

    pub(crate) fn start_wallet_create(&mut self) {
        self.reset_wallet_form();
        self.state.wallets.mode = WalletsMode::Create;
    }

    pub(crate) fn start_wallet_rename(&mut self) {
        let Some(name) = self.selected_wallet().map(|wallet| wallet.name.clone()) else {
            self.state.wallets.error = Some("Nessun wallet selezionato.".to_string());
            return;
        };
        self.reset_wallet_form();
        self.state.wallets.form.name.set_value(name);
        self.state.wallets.mode = WalletsMode::Rename;
        self.state.wallets.form.focus = WalletFormField::Name;
        self.state.wallets.form.update_focus();
    }
    pub(crate) fn selected_wallet(&self) -> Option<&api_types::vault::WalletView> {
        let indices = wallets_visible_indices(&self.state);
        let index = indices.get(self.state.wallets.selected).copied()?;
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.wallets.get(index))
    }

    pub(crate) fn select_wallet_by_id(&mut self, wallet_id: uuid::Uuid) {
        let Some(snapshot) = &self.state.snapshot else {
            return;
        };
        let indices = wallets_visible_indices(&self.state);
        if let Some(pos) = indices.iter().position(|idx| {
            snapshot
                .wallets
                .get(*idx)
                .map(|wallet| wallet.id == wallet_id)
                .unwrap_or(false)
        }) {
            self.state.wallets.selected = pos;
        }
    }

    pub(crate) fn toggle_wallets_show_archived(&mut self) {
        self.state.wallets.show_archived = !self.state.wallets.show_archived;
        // Clamp selection after toggling
        let len = wallets_visible_indices(&self.state).len();
        if len == 0 {
            self.state.wallets.selected = 0;
        } else if self.state.wallets.selected >= len {
            self.state.wallets.selected = len - 1;
        }
    }
}
