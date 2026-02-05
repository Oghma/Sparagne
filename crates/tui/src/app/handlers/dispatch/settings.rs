//! Settings section dispatch handling (Categories, Vault, Members, Preferences tabs).

use crate::app::state::{
    AliasFocus, CategoriesMode, MemberFormField, MembersMode, PreferencesField, SettingsTab,
    VaultMode,
};
use crate::app::App;
use crate::error::Result;
use crate::ui::keymap::AppAction;

impl App {
    /// Dispatches actions for the Settings section.
    pub(crate) async fn dispatch_settings(&mut self, action: AppAction) -> Result<bool> {
        match action {
            AppAction::Submit => self.dispatch_settings_submit().await,
            AppAction::Backspace => self.dispatch_settings_backspace(),
            AppAction::Up => self.dispatch_settings_up(),
            AppAction::Down => self.dispatch_settings_down(),
            AppAction::Left => self.dispatch_settings_left(),
            AppAction::Right => self.dispatch_settings_right(),
            _ => Ok(false),
        }
    }

    async fn dispatch_settings_submit(&mut self) -> Result<bool> {
        match self.state.settings_tab {
            SettingsTab::Categories => {
                self.handle_categories_submit().await?;
                Ok(true)
            }
            SettingsTab::Members => {
                self.handle_members_submit().await?;
                Ok(true)
            }
            SettingsTab::Vault => {
                self.handle_vault_submit().await?;
                Ok(true)
            }
            SettingsTab::Preferences => Ok(false),
        }
    }

    fn dispatch_settings_backspace(&mut self) -> Result<bool> {
        match self.state.settings_tab {
            SettingsTab::Categories => {
                if self.state.categories.mode == CategoriesMode::Aliases
                    && self.state.categories.aliases.focus == AliasFocus::Input
                {
                    self.state.categories.aliases.input.pop();
                    Ok(true)
                } else if matches!(
                    self.state.categories.mode,
                    CategoriesMode::Create | CategoriesMode::Rename
                ) {
                    self.backspace_category_form();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            SettingsTab::Members => {
                if self.state.members.mode == MembersMode::Form
                    && self.state.members.form.focus == MemberFormField::Username
                {
                    self.state.members.form.username.pop();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            SettingsTab::Vault => {
                self.backspace_vault_form();
                Ok(true)
            }
            SettingsTab::Preferences => Ok(false),
        }
    }

    fn dispatch_settings_up(&mut self) -> Result<bool> {
        match self.state.settings_tab {
            SettingsTab::Categories => {
                match self.state.categories.mode {
                    CategoriesMode::List | CategoriesMode::Create | CategoriesMode::Rename => {
                        self.categories_select_prev();
                    }
                    CategoriesMode::Merge => {
                        self.category_merge_select_prev();
                    }
                    CategoriesMode::Aliases => {
                        if self.state.categories.aliases.focus == AliasFocus::List {
                            self.category_alias_select_prev();
                        }
                    }
                }
                Ok(true)
            }
            SettingsTab::Members => {
                match self.state.members.mode {
                    MembersMode::Form => {
                        if self.state.members.form.focus == MemberFormField::Role {
                            self.cycle_member_role(false);
                        }
                    }
                    MembersMode::List => {
                        self.members_select_prev();
                    }
                }
                Ok(true)
            }
            SettingsTab::Vault => {
                if self.state.vault_ui.mode == VaultMode::Defaults {
                    self.defaults_select_prev();
                    Ok(true)
                } else if self.state.vault_ui.mode == VaultMode::Select {
                    self.vaults_select_prev();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            SettingsTab::Preferences => {
                self.state.preferences.focus = self.state.preferences.focus.prev();
                Ok(true)
            }
        }
    }

    fn dispatch_settings_down(&mut self) -> Result<bool> {
        match self.state.settings_tab {
            SettingsTab::Categories => {
                match self.state.categories.mode {
                    CategoriesMode::List | CategoriesMode::Create | CategoriesMode::Rename => {
                        self.categories_select_next();
                    }
                    CategoriesMode::Merge => {
                        self.category_merge_select_next();
                    }
                    CategoriesMode::Aliases => {
                        if self.state.categories.aliases.focus == AliasFocus::List {
                            self.category_alias_select_next();
                        }
                    }
                }
                Ok(true)
            }
            SettingsTab::Members => {
                match self.state.members.mode {
                    MembersMode::Form => {
                        if self.state.members.form.focus == MemberFormField::Role {
                            self.cycle_member_role(true);
                        }
                    }
                    MembersMode::List => {
                        self.members_select_next();
                    }
                }
                Ok(true)
            }
            SettingsTab::Vault => {
                if self.state.vault_ui.mode == VaultMode::Defaults {
                    self.defaults_select_next();
                    Ok(true)
                } else if self.state.vault_ui.mode == VaultMode::Select {
                    self.vaults_select_next();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            SettingsTab::Preferences => {
                self.state.preferences.focus = self.state.preferences.focus.next();
                Ok(true)
            }
        }
    }

    fn dispatch_settings_left(&mut self) -> Result<bool> {
        if self.state.settings_tab == SettingsTab::Preferences
            && self.state.preferences.focus == PreferencesField::Density
        {
            self.cycle_density_prev();
            Ok(true)
        } else {
            self.settings_prev_tab();
            Ok(true)
        }
    }

    fn dispatch_settings_right(&mut self) -> Result<bool> {
        if self.state.settings_tab == SettingsTab::Preferences
            && self.state.preferences.focus == PreferencesField::Density
        {
            self.cycle_density_next();
            Ok(true)
        } else {
            self.settings_next_tab();
            Ok(true)
        }
    }
}
