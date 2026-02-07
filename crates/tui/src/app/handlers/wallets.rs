use super::super::*;

use crate::text::{TextKey, t};

impl App {
    pub(crate) fn wallets_select_next(&mut self) {
        let count = wallets_visible_indices(&self.state).len();
        SelectableWithCount::new(&mut self.state.wallets, count).select_next();
    }

    pub(crate) fn wallets_select_prev(&mut self) {
        let count = wallets_visible_indices(&self.state).len();
        SelectableWithCount::new(&mut self.state.wallets, count).select_prev();
    }

    pub(crate) fn start_wallet_create(&mut self) {
        self.reset_wallet_form();
        self.state.wallets.mode = EntityListMode::Create;
    }

    pub(crate) fn start_wallet_rename(&mut self) {
        let Some(name) = self.selected_wallet().map(|wallet| wallet.name.clone()) else {
            self.state.wallets.error =
                Some(t(self.state.locale, TextKey::ValidationNoWalletSelected).to_string());
            return;
        };
        self.reset_wallet_form();
        self.state.wallets.form.name.set_value(name);
        self.state.wallets.mode = EntityListMode::Rename;
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
        self.state.wallets.toggle_show_archived();
        let count = wallets_visible_indices(&self.state).len();
        SelectableWithCount::new(&mut self.state.wallets, count).clamp_selection();
    }
}
