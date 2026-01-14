use api_types::membership::MembershipRole;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) enum Command {
    Start {
        code: Option<String>,
    },
    Home,
    Help,
    Categories,
    MembersList,
    MembersAdd {
        username: String,
        role: MembershipRole,
    },
    MembersRemove {
        username: String,
    },
    MembersHelp,
    FlowMembersList {
        flow: String,
    },
    FlowMembersAdd {
        flow: String,
        username: String,
        role: MembershipRole,
    },
    FlowMembersRemove {
        flow: String,
        username: String,
    },
    FlowMembersHelp,
    MergeCategory {
        confirm: bool,
        from: String,
        into: String,
    },
    MergeCategoryHelp,
    VaultDelete {
        confirm: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallbackAction {
    NavHome,
    NavWizard,
    ShowList,
    HomePair,
    HomePickWallet,
    HomePickFlow,
    HomeExpense,
    HomeIncome,
    HomeRefund,
    HomeStats,
    WizClose,
    WizPickWallet,
    WizPickFlow,
    WizInput,
    WizCatNone,
    WizCatReset,
    WizCatIndex(usize),
    WizRecent(Uuid),
    PrefsToggleVoided,
    ListNext,
    ListPrev,
    WalletSet(Uuid),
    FlowSet(Uuid),
    TxDetail(Uuid),
    TxVoid(Uuid),
    TxEdit(Uuid),
    TxEditAmount(Uuid),
    TxEditNote(Uuid),
    TxRepeat(Uuid),
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
        "/members" => parse_members_command(arg.as_deref()),
        "/flow_members" => parse_flow_members_command(arg.as_deref()),
        "/merge_category" => parse_merge_category(arg.as_deref()),
        "/vault_delete" => parse_vault_delete(arg.as_deref()),
        _ => None,
    }
}

pub(crate) fn parse_callback_action(data: &str) -> Option<CallbackAction> {
    let action = match data {
        "nav:home" => CallbackAction::NavHome,
        "nav:wizard" => CallbackAction::NavWizard,
        "nav:list" | "home:list" => CallbackAction::ShowList,
        "home:pair" => CallbackAction::HomePair,
        "home:pick_wallet" => CallbackAction::HomePickWallet,
        "home:pick_flow" => CallbackAction::HomePickFlow,
        "home:expense" => CallbackAction::HomeExpense,
        "home:income" => CallbackAction::HomeIncome,
        "home:refund" => CallbackAction::HomeRefund,
        "home:stats" => CallbackAction::HomeStats,
        "wiz:close" => CallbackAction::WizClose,
        "wiz:pick_wallet" => CallbackAction::WizPickWallet,
        "wiz:pick_flow" => CallbackAction::WizPickFlow,
        "wiz:input" => CallbackAction::WizInput,
        "wiz:cat:none" => CallbackAction::WizCatNone,
        "wiz:cat:reset" => CallbackAction::WizCatReset,
        "prefs:toggle_voided" => CallbackAction::PrefsToggleVoided,
        "list:next" => CallbackAction::ListNext,
        "list:prev" => CallbackAction::ListPrev,
        "noop" => CallbackAction::Noop,
        _ => {
            if let Some(idx) = parse_usize_suffix(data, "wiz:cat:") {
                return Some(CallbackAction::WizCatIndex(idx));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "wiz:recent:") {
                return Some(CallbackAction::WizRecent(tx_id));
            }
            if let Some(wallet_id) = parse_uuid_suffix(data, "wallet:set:") {
                return Some(CallbackAction::WalletSet(wallet_id));
            }
            if let Some(flow_id) = parse_uuid_suffix(data, "flow:set:") {
                return Some(CallbackAction::FlowSet(flow_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:detail:") {
                return Some(CallbackAction::TxDetail(tx_id));
            }
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:void:") {
                return Some(CallbackAction::TxVoid(tx_id));
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
            if let Some(tx_id) = parse_uuid_suffix(data, "tx:repeat:") {
                return Some(CallbackAction::TxRepeat(tx_id));
            }
            return None;
        }
    };
    Some(action)
}

pub(crate) fn looks_like_quick_add(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('r')
        || trimmed.starts_with('R')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn parse_vault_delete(arg: Option<&str>) -> Option<Command> {
    let confirm = arg
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .is_some_and(|trimmed| trimmed.eq_ignore_ascii_case("confirm"));
    Some(Command::VaultDelete { confirm })
}

fn parse_merge_category(arg: Option<&str>) -> Option<Command> {
    let Some(trimmed) = arg.map(str::trim).filter(|trimmed| !trimmed.is_empty()) else {
        return Some(Command::MergeCategoryHelp);
    };

    let (confirm, rest) = if let Some(rest) = trimmed.strip_prefix("confirm ") {
        (true, rest)
    } else {
        (false, trimmed)
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Command::MergeCategoryHelp);
    }

    let Some((from, into)) = rest.split_once("->") else {
        return Some(Command::MergeCategoryHelp);
    };

    let from = from.trim();
    let into = into.trim();
    if from.is_empty() || into.is_empty() {
        return Some(Command::MergeCategoryHelp);
    }

    Some(Command::MergeCategory {
        confirm,
        from: from.to_string(),
        into: into.to_string(),
    })
}

fn parse_usize_suffix(data: &str, prefix: &str) -> Option<usize> {
    data.strip_prefix(prefix)
        .and_then(|value| value.parse::<usize>().ok())
}

fn parse_uuid_suffix(data: &str, prefix: &str) -> Option<Uuid> {
    data.strip_prefix(prefix)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn parse_members_command(arg: Option<&str>) -> Option<Command> {
    let Some(arg) = arg else {
        return Some(Command::MembersList);
    };
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Some(Command::MembersList);
    }

    let mut parts = trimmed.split_whitespace();
    let action = parts.next().unwrap_or("");
    match action {
        "list" => Some(Command::MembersList),
        "add" => {
            let Some(username) = parts.next() else {
                return Some(Command::MembersHelp);
            };
            let Some(role_raw) = parts.next() else {
                return Some(Command::MembersHelp);
            };
            if parts.next().is_some() {
                return Some(Command::MembersHelp);
            }
            let Some(role) = parse_membership_role(role_raw) else {
                return Some(Command::MembersHelp);
            };
            Some(Command::MembersAdd {
                username: username.to_string(),
                role,
            })
        }
        "remove" | "rm" => {
            let Some(username) = parts.next() else {
                return Some(Command::MembersHelp);
            };
            if parts.next().is_some() {
                return Some(Command::MembersHelp);
            }
            Some(Command::MembersRemove {
                username: username.to_string(),
            })
        }
        _ => Some(Command::MembersHelp),
    }
}

fn parse_flow_members_command(arg: Option<&str>) -> Option<Command> {
    let Some(arg) = arg else {
        return Some(Command::FlowMembersHelp);
    };
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Some(Command::FlowMembersHelp);
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let action = parts.first().copied().unwrap_or("");

    match action {
        "list" => {
            if parts.len() < 2 {
                return Some(Command::FlowMembersHelp);
            }
            let flow = parts[1..].join(" ");
            Some(Command::FlowMembersList { flow })
        }
        "add" => {
            if parts.len() < 4 {
                return Some(Command::FlowMembersHelp);
            }
            let role_raw = parts[parts.len() - 1];
            let username = parts[parts.len() - 2];
            let flow = parts[1..parts.len() - 2].join(" ");
            if flow.trim().is_empty() {
                return Some(Command::FlowMembersHelp);
            }
            let Some(role) = parse_membership_role(role_raw) else {
                return Some(Command::FlowMembersHelp);
            };
            Some(Command::FlowMembersAdd {
                flow,
                username: username.to_string(),
                role,
            })
        }
        "remove" | "rm" => {
            if parts.len() < 3 {
                return Some(Command::FlowMembersHelp);
            }
            let username = parts[parts.len() - 1];
            let flow = parts[1..parts.len() - 1].join(" ");
            if flow.trim().is_empty() {
                return Some(Command::FlowMembersHelp);
            }
            Some(Command::FlowMembersRemove {
                flow,
                username: username.to_string(),
            })
        }
        _ => Some(Command::FlowMembersList {
            flow: trimmed.to_string(),
        }),
    }
}

fn parse_membership_role(value: &str) -> Option<MembershipRole> {
    match value.to_lowercase().as_str() {
        "owner" => Some(MembershipRole::Owner),
        "editor" => Some(MembershipRole::Editor),
        "viewer" | "view" => Some(MembershipRole::Viewer),
        _ => None,
    }
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
    fn parse_command_members_add() {
        let cmd = parse_command("/members add alice owner");
        match cmd {
            Some(Command::MembersAdd { username, role }) => {
                assert_eq!(username, "alice");
                assert_eq!(role, MembershipRole::Owner);
            }
            _ => panic!("expected members add"),
        }
    }

    #[test]
    fn parse_command_flow_members_list() {
        let cmd = parse_command("/flow_members list Main Flow");
        match cmd {
            Some(Command::FlowMembersList { flow }) => {
                assert_eq!(flow, "Main Flow");
            }
            _ => panic!("expected flow members list"),
        }
    }

    #[test]
    fn parse_callback_action_wallet_set() {
        let id = "00000000-0000-0000-0000-000000000000";
        let action = parse_callback_action(&format!("wallet:set:{id}"));
        assert_eq!(
            action,
            Some(CallbackAction::WalletSet(Uuid::parse_str(id).unwrap()))
        );
    }

    #[test]
    fn parse_callback_action_wiz_category() {
        let action = parse_callback_action("wiz:cat:3");
        assert_eq!(action, Some(CallbackAction::WizCatIndex(3)));
    }

    #[test]
    fn parse_callback_action_home_list() {
        let action = parse_callback_action("home:list");
        assert_eq!(action, Some(CallbackAction::ShowList));
    }
}
