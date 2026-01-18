use api_types::transaction::TransactionKind;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) enum Command {
    Start { code: Option<String> },
    Home,
    Help,
    Categories,
    Export,
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallbackAction {
    // Navigation
    NavHome,

    // Home actions
    StartExpense,
    StartIncome,
    ShowHistory,
    ShowStats,
    ShowHelp,
    PickWallet,
    PickFlow,

    // Wallet/Flow selection
    WalletSet(Uuid),
    FlowSet(Uuid),

    // List
    ListNext,
    ListPrev,
    ToggleVoided,
    ListShowFilters,                         // Show filter menu
    ListFilterKind(Option<TransactionKind>), // Set kind filter (None = all)
    ListFilterClear,                         // Clear all filters

    // Transaction actions
    TxDetail(usize),     // 1-based index in list
    TxDetailById(Uuid),  // Show detail by UUID (for back navigation)
    TxVoidConfirm(Uuid), // Show void confirmation
    TxVoid(Uuid),        // Execute void
    TxRepeat(Uuid),      // Repeat transaction
    TxEdit(Uuid),
    TxEditAmount(Uuid),
    TxEditNote(Uuid),

    // Wizard
    WizardInput,
    WizardCancel,
    WizardPickWallet,
    WizardPickFlow,

    // Templates
    TemplateList,          // Show template list
    TemplateUse(usize),    // Use template by index
    TemplateDelete(usize), // Delete template by index
    TemplateCreate,        // Start template creation

    Noop,
}

pub(crate) fn parse_command(text: &str) -> Option<Command> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().map(|s| s.to_string());

    match cmd {
        "/start" => Some(Command::Start { code: arg }),
        "/home" => Some(Command::Home),
        "/help" => Some(Command::Help),
        "/categories" => Some(Command::Categories),
        "/export" => Some(Command::Export),
        "/template" | "/templates" => Some(Command::Template),
        _ => None,
    }
}

pub(crate) fn parse_callback_action(data: &str) -> Option<CallbackAction> {
    let action = match data {
        // Navigation
        "nav:home" => CallbackAction::NavHome,

        // Home actions
        "home:expense" => CallbackAction::StartExpense,
        "home:income" => CallbackAction::StartIncome,
        "home:history" | "nav:list" | "home:list" => CallbackAction::ShowHistory,
        "home:stats" => CallbackAction::ShowStats,
        "home:help" => CallbackAction::ShowHelp,
        "home:wallet" => CallbackAction::PickWallet,
        "home:flow" => CallbackAction::PickFlow,

        // List
        "list:next" => CallbackAction::ListNext,
        "list:prev" => CallbackAction::ListPrev,
        "list:toggle_voided" | "prefs:toggle_voided" => CallbackAction::ToggleVoided,
        "list:filters" => CallbackAction::ListShowFilters,
        "list:filter:kind:all" => CallbackAction::ListFilterKind(None),
        "list:filter:kind:expense" => {
            CallbackAction::ListFilterKind(Some(TransactionKind::Expense))
        }
        "list:filter:kind:income" => CallbackAction::ListFilterKind(Some(TransactionKind::Income)),
        "list:filter:clear" => CallbackAction::ListFilterClear,

        // Wizard
        "wiz:input" => CallbackAction::WizardInput,
        "wiz:cancel" => CallbackAction::WizardCancel,
        "wiz:wallet" => CallbackAction::WizardPickWallet,
        "wiz:flow" => CallbackAction::WizardPickFlow,

        // Templates
        "tpl:list" => CallbackAction::TemplateList,
        "tpl:create" => CallbackAction::TemplateCreate,

        "noop" => CallbackAction::Noop,

        _ => {
            // Wallet/Flow selection
            if let Some(wallet_id) = parse_uuid_suffix(data, "wallet:set:") {
                return Some(CallbackAction::WalletSet(wallet_id));
            }
            if let Some(flow_id) = parse_uuid_suffix(data, "flow:set:") {
                return Some(CallbackAction::FlowSet(flow_id));
            }

            // Transaction detail by index (1-based)
            if let Some(idx) = parse_usize_suffix(data, "tx:detail:") {
                return Some(CallbackAction::TxDetail(idx));
            }

            // Transaction detail by UUID (for back navigation)
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:detail_id:") {
                return Some(CallbackAction::TxDetailById(tx_id));
            }

            // Transaction actions by UUID
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:void_confirm:") {
                return Some(CallbackAction::TxVoidConfirm(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:void:") {
                return Some(CallbackAction::TxVoid(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:repeat:") {
                return Some(CallbackAction::TxRepeat(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:edit:") {
                return Some(CallbackAction::TxEdit(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:edit_amount:") {
                return Some(CallbackAction::TxEditAmount(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:edit_note:") {
                return Some(CallbackAction::TxEditNote(tx_id));
            }

            // Template use/delete by index
            if let Some(idx) = parse_usize_suffix(data, "tpl:use:") {
                return Some(CallbackAction::TemplateUse(idx));
            }
            if let Some(idx) = parse_usize_suffix(data, "tpl:delete:") {
                return Some(CallbackAction::TemplateDelete(idx));
            }

            return None;
        }
    };
    Some(action)
}

/// Checks if the text looks like a quick-add message.
/// Patterns: `12.50 ...`, `-12.50 ...`, `+12.50 ...`
pub(crate) fn looks_like_quick_add(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn parse_usize_suffix(data: &str, prefix: &str) -> Option<usize> {
    data.strip_prefix(prefix)
        .and_then(|value| value.parse::<usize>().ok())
}

fn parse_uuid_suffix(data: &str, prefix: &str) -> Option<Uuid> {
    data.strip_prefix(prefix)
        .and_then(|value| Uuid::parse_str(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_start_with_code() {
        let cmd = parse_command("/start abc");
        match cmd {
            Some(Command::Start { code }) => {
                assert_eq!(code.as_deref(), Some("abc"));
            }
            _ => panic!("expected start command"),
        }
    }

    #[test]
    fn parse_command_home() {
        let cmd = parse_command("/home");
        assert!(matches!(cmd, Some(Command::Home)));
    }

    #[test]
    fn parse_command_help() {
        let cmd = parse_command("/help");
        assert!(matches!(cmd, Some(Command::Help)));
    }

    #[test]
    fn parse_command_categories() {
        let cmd = parse_command("/categories");
        assert!(matches!(cmd, Some(Command::Categories)));
    }

    #[test]
    fn parse_command_unknown_returns_none() {
        let cmd = parse_command("/unknown");
        assert!(cmd.is_none());
    }

    #[test]
    fn parse_callback_action_wallet_set() {
        let id = "00000000-0000-0000-0000-000000000000";
        let action = parse_callback_action(&format!("wallet:set:{id}"));
        let parsed = Uuid::parse_str(id).unwrap();
        assert_eq!(action, Some(CallbackAction::WalletSet(parsed)));
    }

    #[test]
    fn parse_callback_action_tx_detail_index() {
        let action = parse_callback_action("tx:detail:3");
        assert_eq!(action, Some(CallbackAction::TxDetail(3)));
    }

    #[test]
    fn parse_callback_action_home_history() {
        let action = parse_callback_action("home:history");
        assert_eq!(action, Some(CallbackAction::ShowHistory));
    }

    #[test]
    fn looks_like_quick_add_positive() {
        assert!(looks_like_quick_add("12.50 coffee"));
        assert!(looks_like_quick_add("+100 salary"));
        assert!(looks_like_quick_add("-50 expense"));
    }

    #[test]
    fn looks_like_quick_add_negative() {
        assert!(!looks_like_quick_add("hello world"));
        assert!(!looks_like_quick_add("/start"));
    }
}
