//! Category selection and cycling for transactions.
//!
//! Contains methods for category selection in transaction forms and quick-add ambiguous cycling.

use crate::{
    app::{
        format::map_currency,
        resolve::{resolve_category_matches, resolve_flow_matches, resolve_wallet_matches},
        App, QuickAddAmbiguous, QuickAddAmbiguousKind, Section, TransactionsMode,
    },
    quick_add::parse as parse_quick_add,
};

impl App {
    pub(crate) fn select_category_next(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let next = match form.category_index {
            Some(idx) => (idx + 1) % categories.len(),
            None => 0,
        };
        form.category_index = Some(next);
        form.category.set_value(categories[next].clone());
    }

    pub(crate) fn select_category_prev(&mut self) {
        let categories = self.state.transactions.recent_categories.clone();
        if categories.is_empty() {
            return;
        }
        let form = &mut self.state.transactions.form;
        let prev = match form.category_index {
            Some(idx) => (idx + categories.len() - 1) % categories.len(),
            None => categories.len() - 1,
        };
        form.category_index = Some(prev);
        form.category.set_value(categories[prev].clone());
    }

    /// Cycle through ambiguous options in quick-add input (Ctrl+R)
    pub(crate) fn cycle_quick_add_ambiguous(&mut self) {
        // Only active in transactions list with quick_active
        if self.state.section != Section::Transactions
            || self.state.transactions.mode != TransactionsMode::List
            || !self.state.transactions.quick_active
        {
            return;
        }

        let currency = self
            .state
            .vault
            .as_ref()
            .and_then(|v| v.currency.as_ref())
            .map(map_currency)
            .unwrap_or(engine::Currency::Eur);

        let input = self.state.transactions.quick_input.as_str();
        let parsed = match parse_quick_add(input, currency) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Detect ambiguous matches
        let category_matches = parsed
            .category
            .as_ref()
            .map(|c| resolve_category_matches(&self.state, c))
            .unwrap_or_default();
        let wallet_matches = parsed
            .wallet
            .as_ref()
            .map(|w| resolve_wallet_matches(&self.state, w))
            .unwrap_or_default();
        let flow_matches = parsed
            .flow
            .as_ref()
            .map(|f| resolve_flow_matches(&self.state, f))
            .unwrap_or_default();

        // Priority: cycle category first, then wallet, then flow
        if category_matches.len() > 1 {
            self.cycle_or_init_ambiguous(
                QuickAddAmbiguousKind::Category,
                parsed.category.as_deref().unwrap_or(""),
                category_matches,
            );
        } else if wallet_matches.len() > 1 {
            self.cycle_or_init_ambiguous(
                QuickAddAmbiguousKind::Wallet,
                parsed.wallet.as_deref().unwrap_or(""),
                wallet_matches,
            );
        } else if flow_matches.len() > 1 {
            self.cycle_or_init_ambiguous(
                QuickAddAmbiguousKind::Flow,
                parsed.flow.as_deref().unwrap_or(""),
                flow_matches,
            );
        }
    }

    fn cycle_or_init_ambiguous(
        &mut self,
        kind: QuickAddAmbiguousKind,
        query: &str,
        options: Vec<(uuid::Uuid, String)>,
    ) {
        if let Some(amb) = &mut self.state.transactions.quick_ambiguous
            && amb.kind == kind
        {
            // Cycle to next option
            amb.cycle_next();
            return;
        }

        // Initialize new ambiguous state
        self.state.transactions.quick_ambiguous =
            Some(QuickAddAmbiguous::new(kind, query.to_string(), options));
    }
}
