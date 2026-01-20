use engine::Currency as EngineCurrency;

use crate::i18n::{self, TextKey};

pub(crate) fn api_currency_to_engine(currency: api_types::Currency) -> EngineCurrency {
    match currency {
        api_types::Currency::Eur => EngineCurrency::Eur,
    }
}

pub(crate) fn flow_display_name(locale: i18n::Locale, is_unallocated: bool, name: &str) -> &str {
    if is_unallocated {
        i18n::t(locale, TextKey::UnallocatedFlow)
    } else {
        name
    }
}
