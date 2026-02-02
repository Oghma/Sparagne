mod en;
mod it;

/// Supported locales for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    It,
    En,
}

impl Locale {
    /// Parses a locale string into a `Locale` variant.
    ///
    /// Supports common locale identifiers like "en", "en_US", "en_GB", "it",
    /// "it_IT". Defaults to `Locale::It` for unrecognized values.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "en" | "en_us" | "en_gb" | "english" => Self::En,
            _ => Self::It,
        }
    }
}

/// Text keys for all user-facing strings in the TUI.
///
/// Keys are organized by category:
/// - Section: navigation/section names
/// - Label: form field labels
/// - Title: screen/modal titles
/// - Validation: validation error messages
/// - Empty: empty state messages
/// - Action: button/action labels
/// - Hint: contextual hints
/// - Success/Error: operation feedback
/// - Misc: other strings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKey {
    // Sections (8)
    SectionHome,
    SectionTransactions,
    SectionWallets,
    SectionFlows,
    SectionCategories,
    SectionMembers,
    SectionVault,
    SectionStats,

    // Form Labels (13)
    LabelAmount,
    LabelWallet,
    LabelFlow,
    LabelCategory,
    LabelNote,
    LabelOccurredAt,
    LabelFrom,
    LabelTo,
    LabelName,
    LabelUsername,
    LabelRole,
    LabelOpeningBalance,
    LabelCap,

    // Form Titles (16)
    TitleNewExpense,
    TitleNewIncome,
    TitleNewRefund,
    TitleNewTransfer,
    TitleEditTransaction,
    TitleNewWallet,
    TitleEditWallet,
    TitleNewFlow,
    TitleEditFlow,
    TitleNewCategory,
    TitleEditCategory,
    TitleNewMember,
    TitleEditMember,
    TitleNewVault,
    TitleVaultDefaults,
    TitleSelectVault,

    // Validation Errors (18)
    ValidationRequired,
    ValidationAmountRequired,
    ValidationAmountInvalid,
    ValidationAmountPositive,
    ValidationAmountNegative,
    ValidationDateRequired,
    ValidationDateInvalid,
    ValidationDateInvalidTimezone,
    ValidationLengthMin,
    ValidationLengthMax,
    ValidationWalletRequired,
    ValidationFlowRequired,
    ValidationCategoryRequired,
    ValidationNameRequired,
    ValidationUsernameRequired,
    ValidationRoleRequired,
    ValidationFromRequired,
    ValidationToRequired,

    // Empty States (8)
    EmptyTransactions,
    EmptyWallets,
    EmptyFlows,
    EmptyCategories,
    EmptyMembers,
    EmptyVaults,
    EmptyStats,
    EmptyResults,

    // Actions (12)
    ActionSave,
    ActionCancel,
    ActionCreate,
    ActionEdit,
    ActionDelete,
    ActionArchive,
    ActionVoid,
    ActionRefund,
    ActionTransfer,
    ActionConfirm,
    ActionBack,
    ActionRefresh,

    // Hints - General (8)
    HintPressEnter,
    HintPressEsc,
    HintPressTab,
    HintSelectWithArrows,
    HintTypeToSearch,
    HintLoadingData,
    HintNoSelection,
    HintConfirmDelete,

    // Hints - Footer Navigation
    HintHome,
    HintTransactions,
    HintAccounts,
    HintAnalytics,
    HintSettings,

    // Hints - Footer Actions
    HintQuickAdd,
    HintHelp,
    HintCreate,
    HintEdit,
    HintDelete,
    HintSave,
    HintCancel,
    HintBack,
    HintRefresh,
    HintAdd,
    HintToggle,
    HintCategorize,
    HintExit,

    // Help Overlay
    HelpTitle,
    HelpCloseHelp,
    HelpGlobal,
    HelpNavigation,
    HelpCommonActions,
    HelpQuickAddTxn,
    HelpNewTxnModal,
    HelpCommandPalette,
    HelpSearch,
    HelpShowHelp,
    HelpNextSubTab,
    HelpPrevSubTab,
    HelpNavigateList,
    HelpOpenDetails,
    HelpBackClose,
    HelpEditSelected,
    HelpDeleteSelected,
    HelpNavigateFeed,
    HelpGoToTransactions,
    HelpGoToAccounts,
    HelpGoToAnalytics,
    HelpGoToSettings,
    HelpNewIncome,
    HelpNewRefund,
    HelpToggleFilters,
    HelpGroupTxns,
    HelpScopeWallet,
    HelpScopeFlow,
    HelpClearFilters,
    HelpDeleteTxn,
    HelpUndoDelete,
    HelpToggleVoided,
    HelpNextPrevPage,
    HelpVisualMode,
    HelpToggleVisual,
    HelpSelectTxn,
    HelpExitVisual,
    HelpDetailView,
    HelpEditTxn,
    HelpRepeatTxn,
    HelpVoidTxn,
    HelpForm,
    HelpNextField,
    HelpChangeValue,
    HelpFilters,
    HelpToggleType,
    HelpToggleScope,
    HelpApply,
    HelpJumpSubTab,
    HelpSourcesWallets,
    HelpCreateWallet,
    HelpRenameWallet,
    HelpDeleteArchive,
    HelpViewDetails,
    HelpEnvelopesFlows,
    HelpCreateEnvelope,
    HelpRenameEnvelope,
    HelpChangeMode,
    HelpGoals,
    HelpComingSoon,
    HelpRefreshData,
    HelpSwitchView,
    HelpCashSpendWorth,
    HelpChangePeriod,
    HelpCreateCategory,
    HelpRenameCategory,
    HelpManageAliases,
    HelpMergeCategories,
    HelpAliases,
    HelpSwitchFocus,
    HelpDeleteAlias,
    HelpAddSave,
    HelpVault,
    HelpCreateVault,
    HelpSelectVault,
    HelpMembers,
    HelpAddMember,
    HelpEditMember,
    HelpRemoveMember,
    HelpVaultMembers,
    HelpFlowSharing,
    HelpChangeFlow,
    HelpChangeRole,

    // Status bar
    StatusOnline,
    StatusOffline,

    // Success Messages (6)
    SuccessCreated,
    SuccessUpdated,
    SuccessDeleted,
    SuccessArchived,
    SuccessVoided,
    SuccessRefreshed,

    // Error Messages (4)
    ErrorGeneric,
    ErrorNetwork,
    ErrorNotFound,
    ErrorUnauthorized,

    // Misc (4)
    MiscYes,
    MiscNo,
    MiscAll,
    MiscNone,
}

/// Returns the localized string for a text key.
#[must_use]
pub fn t(locale: Locale, key: TextKey) -> &'static str {
    match locale {
        Locale::It => it::get(key),
        Locale::En => en::get(key),
    }
}

/// Returns a formatted string with placeholder substitution.
///
/// Placeholders in the form `{name}` are replaced with the corresponding value
/// from the `pairs` slice.
///
/// # Example
///
/// ```ignore
/// let msg = format(Locale::It, TextKey::ValidationLengthMin, &[("min", "3")]);
/// // "Minimo 3 caratteri"
/// ```
#[must_use]
pub fn format(locale: Locale, key: TextKey, pairs: &[(&str, &str)]) -> String {
    let mut result = t(locale, key).to_string();
    for (name, value) in pairs {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

/// Returns the default locale for the TUI.
#[must_use]
pub fn default_locale() -> Locale {
    Locale::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_replaces_placeholders() {
        let result = format(Locale::It, TextKey::ValidationLengthMin, &[("min", "5")]);
        assert!(result.contains('5'));
    }

    #[test]
    fn t_returns_non_empty() {
        assert!(!t(Locale::It, TextKey::SectionHome).is_empty());
    }
}
