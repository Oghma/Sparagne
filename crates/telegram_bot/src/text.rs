use teloxide::types::User;

pub(crate) fn welcome_text(display_name: &str) -> String {
    format!(
        "Benvenuto, {display_name}!\n\nOra puoi inserire voci al volo scrivendo ad esempio:\n\n12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\nImposta i default (wallet/flow) usando i bottoni."
    )
}

pub(crate) fn help_text() -> &'static str {
    "Esempi:\n\n12.50 bar caff\u{e8}\n-12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale (max 1): 12.50 bar #food caff\u{e8}\n\nComandi:\n/home\n/categories\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n/vault_delete\n/vault_delete confirm"
}

pub(crate) fn merge_category_help_text() -> &'static str {
    "Uso:\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>"
}

pub(crate) fn members_help_text() -> &'static str {
    "Uso:\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>"
}

pub(crate) fn flow_members_help_text() -> &'static str {
    "Uso:\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n\nNota: il flow pu\u{f2} contenere spazi."
}

pub(crate) fn display_name_from_telegram(user: &User) -> String {
    if let Some(username) = user.username.as_deref().filter(|u| !u.is_empty()) {
        format!("@{username}")
    } else if !user.first_name.is_empty() {
        user.first_name.clone()
    } else {
        "Sparagne".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_base() -> User {
        User {
            id: teloxide::types::UserId(1),
            is_bot: false,
            first_name: "Mario".to_string(),
            last_name: None,
            username: None,
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        }
    }

    #[test]
    fn display_name_uses_username_when_present() {
        let mut user = user_base();
        user.username = Some("mario".to_string());
        assert_eq!(display_name_from_telegram(&user), "@mario");
    }

    #[test]
    fn display_name_uses_first_name_when_no_username() {
        let user = user_base();
        assert_eq!(display_name_from_telegram(&user), "Mario");
    }

    #[test]
    fn display_name_falls_back_to_default() {
        let mut user = user_base();
        user.first_name.clear();
        assert_eq!(display_name_from_telegram(&user), "Sparagne");
    }

    #[test]
    fn help_text_contains_known_commands() {
        let text = help_text();
        assert!(text.contains("/home"));
        assert!(text.contains("/members"));
        assert!(text.contains("/flow_members"));
        assert!(text.contains("/vault_delete"));
    }

    #[test]
    fn help_text_contains_quick_add_examples() {
        let text = help_text();
        assert!(text.contains("12.50"));
        assert!(text.contains("+1000"));
        assert!(text.contains("r 5.20"));
    }
}
