use super::super::*;

use crate::error::Result;
use api_types::transaction::TransactionKind;

impl App {
    pub(crate) async fn handle_transactions_submit(&mut self) -> Result<()> {
        match self.state.transactions.mode {
            TransactionsMode::List => {
                if self.state.transactions.quick_active {
                    self.submit_quick_add().await
                } else {
                    self.open_transaction_detail().await
                }
            }
            TransactionsMode::Detail => Ok(()),
            TransactionsMode::Edit | TransactionsMode::Form => self.submit_transaction_form().await,
            TransactionsMode::PickWallet => self.apply_wallet_picker().await,
            TransactionsMode::PickFlow => self.apply_flow_picker().await,
            TransactionsMode::TransferWallet => self.submit_transfer_wallet().await,
            TransactionsMode::TransferFlow => self.submit_transfer_flow().await,
            TransactionsMode::Filter => self.apply_filter().await,
        }
    }

    pub(crate) async fn handle_wallets_submit(&mut self) -> Result<()> {
        match self.state.wallets.mode {
            WalletsMode::List => self.open_wallet_detail().await,
            WalletsMode::Detail => Ok(()),
            WalletsMode::Create => self.submit_wallet_create().await,
            WalletsMode::Rename => self.submit_wallet_rename().await,
        }
    }

    pub(crate) async fn handle_flows_submit(&mut self) -> Result<()> {
        match self.state.flows.mode {
            FlowsMode::List => self.open_flow_detail().await,
            FlowsMode::Detail => Ok(()),
            FlowsMode::Create => self.submit_flow_create().await,
            FlowsMode::Rename => self.submit_flow_rename().await,
        }
    }

    pub(crate) async fn handle_categories_submit(&mut self) -> Result<()> {
        match self.state.categories.mode {
            CategoriesMode::List => Ok(()),
            CategoriesMode::Merge => self.submit_category_merge().await,
            CategoriesMode::Create => self.submit_category_create().await,
            CategoriesMode::Rename => self.submit_category_rename().await,
            CategoriesMode::Aliases => self.submit_category_alias_create().await,
        }
    }

    pub(crate) async fn handle_members_submit(&mut self) -> Result<()> {
        match self.state.members.mode {
            MembersMode::List => {
                self.start_member_edit();
                Ok(())
            }
            MembersMode::Form => self.submit_member_form().await,
        }
    }

    pub(crate) async fn handle_vault_submit(&mut self) -> Result<()> {
        match self.state.vault_ui.mode {
            VaultMode::Create => {
                self.submit_vault_create().await?;
            }
            VaultMode::Defaults => {
                self.save_defaults().await?;
            }
            VaultMode::Select => {
                self.submit_vault_select().await?;
            }
            VaultMode::View => {}
        }
        Ok(())
    }

    pub(crate) async fn handle_non_login_key(&mut self, ch: char) -> Result<()> {
        match ch {
            'h' | 'H' => {
                self.state.section = Section::Home;
                self.state.transactions.mode = TransactionsMode::List;
                return Ok(());
            }
            'i' | 'I' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Income).await?;
                }
                return Ok(());
            }
            // Navigation keys - always navigate to section
            't' | 'T' => {
                self.state.section = Section::Transactions;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.transactions.items.is_empty() {
                    self.load_transactions(true).await?;
                }
                return Ok(());
            }
            'w' | 'W' => {
                self.state.section = Section::Wallets;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'f' | 'F' => {
                self.state.section = Section::Flows;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'g' | 'G' => {
                self.state.section = Section::Categories;
                self.state.transactions.mode = TransactionsMode::List;
                self.load_categories().await?;
                return Ok(());
            }
            'v' | 'V' => {
                if self.state.section == Section::Transactions {
                    match self.state.transactions.mode {
                        TransactionsMode::List => self.toggle_visual_mode(),
                        TransactionsMode::Detail => {
                            self.void_transaction().await?;
                        }
                        _ => {}
                    }
                } else {
                    self.state.section = Section::Vault;
                    self.state.transactions.mode = TransactionsMode::List;
                }
                return Ok(());
            }
            'd' | 'D' => {
                if self.state.section == Section::Transactions
                    && matches!(
                        self.state.transactions.mode,
                        TransactionsMode::List | TransactionsMode::Detail
                    )
                {
                    self.open_transaction_delete_dialog();
                    return Ok(());
                }
                if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    if let Some(wallet) = self.selected_wallet()
                        && !wallet.archived
                    {
                        self.open_wallet_archive_dialog();
                    } else {
                        self.toggle_wallet_archive().await?;
                    }
                    return Ok(());
                }
                if self.state.section == Section::Flows && self.state.flows.mode == FlowsMode::List
                {
                    if let Some(flow) = self.selected_flow()
                        && !flow.archived
                    {
                        self.open_flow_archive_dialog();
                    } else {
                        self.toggle_flow_archive().await?;
                    }
                    return Ok(());
                }
                if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::List
                {
                    if let Some(category) = self.selected_category()
                        && !category.archived
                    {
                        self.open_category_archive_dialog();
                    } else {
                        self.toggle_category_archive().await?;
                    }
                    return Ok(());
                }
                if self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    if self.state.snapshot.is_none() {
                        self.refresh_snapshot().await?;
                    }
                    self.start_defaults();
                    return Ok(());
                }
            }
            // Transaction list context actions (use different keys)
            'z' | 'Z' => {
                // Toggle voided visibility in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.include_voided =
                        !self.state.transactions.include_voided;
                    self.load_transactions(true).await?;
                }
                return Ok(());
            }
            '1' => {
                // Open wallet picker in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_wallet_picker();
                } else if self.state.section == Section::Stats {
                    self.stats_set_tab(0);
                }
                return Ok(());
            }
            '2' => {
                // Open flow picker in transactions list
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_flow_picker();
                } else if self.state.section == Section::Stats {
                    self.stats_set_tab(1);
                }
                return Ok(());
            }
            '3' => {
                if self.state.section == Section::Stats {
                    self.stats_set_tab(2);
                }
                return Ok(());
            }
            's' | 'S' => {
                self.state.section = Section::Stats;
                self.state.transactions.mode = TransactionsMode::List;
                self.load_stats().await?;
                return Ok(());
            }
            'R' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Refund).await?;
                }
                return Ok(());
            }
            'r' => {
                if self.state.section == Section::Transactions {
                    if self.state.transactions.mode == TransactionsMode::Detail {
                        self.repeat_transaction().await?;
                    } else if self.state.transactions.mode == TransactionsMode::List {
                        self.load_transactions(true).await?;
                    }
                } else if self.state.section == Section::Stats {
                    self.load_stats().await?;
                } else if self.state.section == Section::Categories {
                    if self.state.categories.mode == CategoriesMode::Aliases {
                        self.reload_category_aliases().await?;
                    } else {
                        self.load_categories().await?;
                    }
                } else if self.state.section == Section::Members {
                    self.load_members().await?;
                } else if self.state.section == Section::Wallets
                    || self.state.section == Section::Flows
                {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            // Quick Add: 'n' = inline, 'N' = modal (new transaction form)
            'n' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.quick_active = true;
                    self.state.transactions.quick_error = None;
                } else if self.state.section == Section::Home {
                    // From Home, go to transactions and open quick add
                    self.state.section = Section::Transactions;
                    self.state.transactions.mode = TransactionsMode::List;
                    if self.state.transactions.items.is_empty() {
                        self.load_transactions(true).await?;
                    }
                    self.state.transactions.quick_active = true;
                    self.state.transactions.quick_error = None;
                }
                return Ok(());
            }
            'N' => {
                // Open full transaction form (modal) for new expense
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.start_transaction_form(TransactionKind::Expense)
                        .await?;
                } else if self.state.section == Section::Home {
                    // From Home, go to transactions and open form
                    self.state.section = Section::Transactions;
                    self.state.transactions.mode = TransactionsMode::List;
                    if self.state.transactions.items.is_empty() {
                        self.load_transactions(true).await?;
                    }
                    self.start_transaction_form(TransactionKind::Expense)
                        .await?;
                }
                return Ok(());
            }
            // Pagination for transactions/stats
            ']' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_next().await?;
                } else if self.state.section == Section::Stats {
                    self.stats_next_month();
                }
                return Ok(());
            }
            '[' => {
                if self.state.section == Section::Transactions {
                    self.load_transactions_prev().await?;
                } else if self.state.section == Section::Stats {
                    self.stats_prev_month();
                }
                return Ok(());
            }
            'j' | 'J' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_next();
                } else if self.state.section == Section::Home {
                    self.home_feed_select_next();
                }
                return Ok(());
            }
            'k' | 'K' => {
                if self.state.section == Section::Transactions {
                    self.state.transactions.select_prev();
                } else if self.state.section == Section::Home {
                    self.home_feed_select_prev();
                }
                return Ok(());
            }
            // 'a' navigates to Accounts section
            'a' => {
                // Navigate to Accounts (Flows/Envelopes section)
                self.state.section = Section::Flows;
                self.state.transactions.mode = TransactionsMode::List;
                if self.state.snapshot.is_none() {
                    self.refresh_snapshot().await?;
                }
                return Ok(());
            }
            'u' | 'U' => {
                if self.handle_undo_hotkey().await? {
                    return Ok(());
                }
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.undo_last_transaction().await?;
                }
                return Ok(());
            }
            ' ' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                    && self.state.transactions.visual_mode
                {
                    self.toggle_visual_selection();
                    return Ok(());
                }
            }
            // 'e' = edit selected item (uniform contract)
            'e' | 'E' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::Detail
                {
                    self.start_transaction_edit().await?;
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.start_wallet_rename();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.start_flow_rename();
                } else if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_rename();
                }
                return Ok(());
            }
            'b' | 'B' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode != TransactionsMode::List
                {
                    match self.state.transactions.mode {
                        TransactionsMode::Detail => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.detail = None;
                        }
                        TransactionsMode::Edit => {
                            self.state.transactions.mode = TransactionsMode::Detail;
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::Form => {
                            if self.state.transactions.form.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.form = TransactionFormState::default();
                        }
                        TransactionsMode::TransferWallet | TransactionsMode::TransferFlow => {
                            if self.state.transactions.transfer.editing_id.is_some() {
                                self.state.transactions.mode = TransactionsMode::Detail;
                            } else {
                                self.state.transactions.mode = TransactionsMode::List;
                            }
                            self.state.transactions.transfer = TransferFormState::default();
                        }
                        TransactionsMode::PickWallet | TransactionsMode::PickFlow => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.picker_index = 0;
                        }
                        TransactionsMode::Filter => {
                            self.state.transactions.mode = TransactionsMode::List;
                            self.state.transactions.filter.error = None;
                        }
                        TransactionsMode::List => {}
                    }
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode != WalletsMode::List
                {
                    self.state.wallets.mode = WalletsMode::List;
                    self.state.wallets.detail = WalletDetailState::default();
                    self.reset_wallet_form();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode != FlowsMode::List
                {
                    self.state.flows.mode = FlowsMode::List;
                    self.state.flows.detail = FlowDetailState::default();
                    self.reset_flow_form();
                } else if self.state.section == Section::Vault
                    && self.state.vault_ui.mode != VaultMode::View
                {
                    self.reset_vault_form();
                    self.state.vault_ui.defaults = DefaultsFormState::default();
                    self.state.vault_ui.mode = VaultMode::View;
                }
                return Ok(());
            }
            'c' | 'C' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    if self.state.transactions.visual_mode {
                        self.open_bulk_category_dialog();
                        return Ok(());
                    }
                    self.clear_filters().await?;
                } else if self.state.section == Section::Wallets
                    && self.state.wallets.mode == WalletsMode::List
                {
                    self.start_wallet_create();
                } else if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::List
                {
                    self.start_flow_create();
                } else if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_create();
                } else if self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.start_vault_create();
                }
                return Ok(());
            }
            'l' | 'L' => {
                if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_aliases().await?;
                } else if self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.start_vault_select().await?;
                }
                return Ok(());
            }
            'x' | 'X' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.state.transactions.include_transfers =
                        !self.state.transactions.include_transfers;
                    self.load_transactions(true).await?;
                } else if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::Aliases
                    && self.state.categories.aliases.focus == AliasFocus::List
                {
                    self.delete_category_alias().await?;
                } else if self.state.section == Section::Vault
                    && self.state.vault_ui.mode == VaultMode::View
                {
                    self.open_vault_delete_dialog();
                }
                return Ok(());
            }
            'm' | 'M' => {
                if self.state.section == Section::Flows
                    && self.state.flows.mode == FlowsMode::Create
                    && self.state.flows.form.focus == FlowFormField::Mode
                {
                    self.cycle_flow_mode();
                    return Ok(());
                }
                if self.state.section == Section::Categories
                    && self.state.categories.mode == CategoriesMode::List
                {
                    self.start_category_merge();
                    return Ok(());
                }
                self.open_members().await?;
                return Ok(());
            }
            '/' => {
                if self.state.section == Section::Transactions
                    && self.state.transactions.mode == TransactionsMode::List
                {
                    self.open_filter();
                }
                return Ok(());
            }
            '?' => {
                if self.state.screen == Screen::Home {
                    self.state.help.active = true;
                }
                return Ok(());
            }
            _ => {}
        }
        Ok(())
    }
}
