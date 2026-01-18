use teloxide::types::User;

use crate::{
    i18n::{self, TextKey},
    state::ScreenContext,
};

pub(crate) fn welcome_text(locale: i18n::Locale, display_name: &str) -> String {
    let template = i18n::t(locale, TextKey::WelcomeTemplate);
    template.replace("{display_name}", display_name)
}

pub(crate) fn help_text(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::HelpText)
}

pub(crate) fn pairing_success(locale: i18n::Locale) -> &'static str {
    i18n::t(locale, TextKey::PairingSuccess)
}

pub(crate) fn first_time_welcome(locale: i18n::Locale, display_name: &str) -> String {
    let welcome = i18n::format(
        locale,
        TextKey::WelcomeFirstTime,
        &[("display_name", display_name)],
    );
    let concepts = i18n::t(locale, TextKey::ConceptsExplanation);
    let quickstart = i18n::t(locale, TextKey::QuickStartGuide);
    format!("{welcome}\n\n{concepts}\n\n{quickstart}")
}

pub(crate) fn contextual_help(locale: i18n::Locale, screen: ScreenContext) -> String {
    let main_text = match screen {
        ScreenContext::Home => i18n::t(locale, TextKey::HelpTextHome),
        ScreenContext::Wizard => i18n::t(locale, TextKey::HelpTextWizard),
        ScreenContext::List => i18n::t(locale, TextKey::HelpTextList),
        ScreenContext::Stats => i18n::t(locale, TextKey::HelpTextStats),
    };
    let footer = i18n::t(locale, TextKey::HelpFooter);
    format!("{main_text}{footer}")
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
    }

    #[test]
    fn help_text_contains_quick_add_examples() {
        let text = help_text(i18n::default_locale());
        assert!(text.contains("12.50"));
        assert!(text.contains("+1000"));
    }
}
