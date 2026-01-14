use teloxide::types::User;

use crate::i18n::{self, TextKey};

pub(crate) fn welcome_text(locale: i18n::Locale, display_name: &str) -> String {
    let template = i18n::t(locale, TextKey::WelcomeTemplate);
    template.replace("{display_name}", display_name)
}

pub(crate) fn help_text(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::HelpText)
}

pub(crate) fn merge_category_help_text(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::MergeCategoryHelp)
}

pub(crate) fn members_help_text(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::MembersHelp)
}

pub(crate) fn flow_members_help_text(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::FlowMembersHelp)
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
        let text = help_text(i18n::default_locale());
        assert!(text.contains("/home"));
        assert!(text.contains("/members"));
        assert!(text.contains("/flow_members"));
        assert!(text.contains("/vault_delete"));
    }

    #[test]
    fn help_text_contains_quick_add_examples() {
        let text = help_text(i18n::default_locale());
        assert!(text.contains("12.50"));
        assert!(text.contains("+1000"));
        assert!(text.contains("r 5.20"));
    }
}
