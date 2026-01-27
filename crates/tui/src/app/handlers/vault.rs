use super::super::*;

impl App {
    pub(crate) fn reset_after_vault_delete(&mut self) {
        self.state.screen = Screen::Login;
        self.state.login.password.clear();
        self.state.login.focus = LoginField::Username;
        self.state.login.message = Some("Vault eliminato.".to_string());
        self.state.vault = None;
        self.state.snapshot = None;
        self.state.section = Section::Home;
        self.state.transactions = TransactionsState::default();
        self.state.wallets = WalletsState::default();
        self.state.flows = FlowsState::default();
        self.state.vault_ui = VaultState::default();
        self.state.categories = CategoriesState::default();
        self.state.members = MembersState::default();
        self.state.stats = StatsState::default();
        self.state.palette = CommandPaletteState::default();
        self.state.help = HelpState::default();
        self.state.toast = None;
        self.state.overlays = OverlayState::default();
        self.state.connection = ConnectionState::default();
        self.state.spinner = SpinnerState::default();
        self.state.last_refresh = None;
        self.state.last_flow_id = None;
        self.state.default_wallet_id = None;
        self.state.default_flow_id = None;
    }

    pub(crate) fn start_vault_create(&mut self) {
        self.reset_vault_form();
        self.state.vault_ui.mode = VaultMode::Create;
    }

    pub(crate) fn vaults_select_next(&mut self) {
        let len = self.state.vault_ui.list.items.len();
        if len == 0 {
            return;
        }
        self.state.vault_ui.list.selected = (self.state.vault_ui.list.selected + 1).min(len - 1);
    }

    pub(crate) fn vaults_select_prev(&mut self) {
        if self.state.vault_ui.list.items.is_empty() {
            return;
        }
        self.state.vault_ui.list.selected = self.state.vault_ui.list.selected.saturating_sub(1);
    }
    pub(crate) fn start_defaults(&mut self) {
        let wallet_ids = self.active_wallet_ids();
        let flow_ids = self.active_flow_ids();

        let wallet_index = match self.state.default_wallet_id {
            Some(default_id) => wallet_ids
                .iter()
                .position(|id| *id == default_id)
                .map(|idx| idx + 1)
                .unwrap_or(0),
            None => 0,
        };
        let flow_index = match self.state.default_flow_id {
            Some(default_id) => flow_ids
                .iter()
                .position(|id| *id == default_id)
                .map(|idx| idx + 1)
                .unwrap_or(0),
            None => 0,
        };

        self.state.vault_ui.defaults = DefaultsFormState {
            wallet_index,
            flow_index,
            focus: DefaultsField::Wallet,
            error: None,
        };
        self.state.vault_ui.mode = VaultMode::Defaults;
    }
    pub(crate) fn defaults_select_next(&mut self) {
        let len = match self.state.vault_ui.defaults.focus {
            DefaultsField::Wallet => self.active_wallets_len() + 1,
            DefaultsField::Flow => self.active_flows_len() + 1,
        };
        if len <= 1 {
            return;
        }
        match self.state.vault_ui.defaults.focus {
            DefaultsField::Wallet => {
                self.state.vault_ui.defaults.wallet_index =
                    (self.state.vault_ui.defaults.wallet_index + 1) % len;
            }
            DefaultsField::Flow => {
                self.state.vault_ui.defaults.flow_index =
                    (self.state.vault_ui.defaults.flow_index + 1) % len;
            }
        }
    }

    pub(crate) fn defaults_select_prev(&mut self) {
        let len = match self.state.vault_ui.defaults.focus {
            DefaultsField::Wallet => self.active_wallets_len() + 1,
            DefaultsField::Flow => self.active_flows_len() + 1,
        };
        if len <= 1 {
            return;
        }
        match self.state.vault_ui.defaults.focus {
            DefaultsField::Wallet => {
                self.state.vault_ui.defaults.wallet_index =
                    (self.state.vault_ui.defaults.wallet_index + len - 1) % len;
            }
            DefaultsField::Flow => {
                self.state.vault_ui.defaults.flow_index =
                    (self.state.vault_ui.defaults.flow_index + len - 1) % len;
            }
        }
    }
}
