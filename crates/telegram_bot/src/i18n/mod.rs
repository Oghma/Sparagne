#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locale {
    It,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextKey {
    WelcomeTemplate,
    HelpText,
    MergeCategoryHelp,
    MembersHelp,
    FlowMembersHelp,
}

pub(crate) fn default_locale() -> Locale {
    Locale::It
}

pub(crate) fn resolve_locale(telegram_language: Option<&str>) -> Locale {
    let Some(code) = telegram_language else {
        return default_locale();
    };
    if code.starts_with("it") {
        Locale::It
    } else {
        default_locale()
    }
}

pub(crate) fn t(locale: Locale, key: TextKey) -> &'static str {
    match (locale, key) {
        (Locale::It, TextKey::WelcomeTemplate) => {
            "Benvenuto, {display_name}!\n\nOra puoi inserire voci al volo scrivendo ad esempio:\n\n12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\nImposta i default (wallet/flow) usando i bottoni."
        }
        (Locale::It, TextKey::HelpText) => {
            "Esempi:\n\n12.50 bar caff\u{e8}\n-12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale (max 1): 12.50 bar #food caff\u{e8}\n\nComandi:\n/home\n/categories\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n/vault_delete\n/vault_delete confirm"
        }
        (Locale::It, TextKey::MergeCategoryHelp) => {
            "Uso:\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>"
        }
        (Locale::It, TextKey::MembersHelp) => {
            "Uso:\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>"
        }
        (Locale::It, TextKey::FlowMembersHelp) => {
            "Uso:\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n\nNota: il flow pu\u{f2} contenere spazi."
        }
    }
}
