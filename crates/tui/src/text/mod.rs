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
#[allow(dead_code)]
pub enum TextKey {
    // Sections (9)
    SectionHome,
    SectionTransactions,
    SectionWallets,
    SectionFlows,
    SectionCategories,
    SectionMembers,
    SectionVault,
    SectionStats,
    SectionPreferences,

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

    // Validation Errors (32)
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
    ValidationTransferSameSource,
    ValidationTransferSameDestination,
    ValidationTransferSameElements,
    ValidationTransferMinimumTwo,
    ValidationWalletInvalid,
    ValidationFlowInvalid,
    ValidationCategoryInvalid,
    ValidationOpeningBalanceInvalid,
    ValidationCapInvalid,
    ValidationCapMustBePositive,
    ValidationOpeningBalanceNonNegative,
    ValidationNoWalletSelected,
    ValidationNoFlowSelected,
    ValidationNoTransactionToVoid,
    ValidationUnallocatedCannotRename,
    ValidationUnallocatedCannotArchive,
    ValidationAlreadyArchived,
    ValidationSystemCategoryImmutable,

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
    HelpNewTransfer,
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

    // Success Messages (24)
    SuccessCreated,
    SuccessUpdated,
    SuccessDeleted,
    SuccessArchived,
    SuccessVoided,
    SuccessRefreshed,
    SuccessTransactionSaved,
    SuccessTransactionUpdated,
    SuccessTransactionVoided,
    SuccessTransactionRepeated,
    SuccessTransferWalletSaved,
    SuccessTransferWalletUpdated,
    SuccessTransferFlowSaved,
    SuccessTransferFlowUpdated,
    SuccessWalletCreated,
    SuccessWalletUpdated,
    SuccessWalletRestored,
    SuccessFlowCreated,
    SuccessFlowUpdated,
    SuccessFlowRestored,
    SuccessCategoryCreated,
    SuccessCategoryUpdated,
    SuccessAliasCreated,
    SuccessAliasDeleted,
    SuccessVaultCreated,
    SuccessVaultSelected,
    SuccessDefaultsSaved,
    SuccessMergeCompleted,
    SuccessMergePreviewOk,

    // Error Messages (28)
    ErrorGeneric,
    ErrorNetwork,
    ErrorNotFound,
    ErrorUnauthorized,
    ErrorSaving,
    ErrorUpdating,
    ErrorDeleting,
    ErrorArchiving,
    ErrorRestoring,
    ErrorVoiding,
    ErrorRepeating,
    ErrorConnection,
    ErrorTransferWallet,
    ErrorTransferFlow,
    ErrorCreateWallet,
    ErrorUpdateWallet,
    ErrorArchiveWallet,
    ErrorRestoreWallet,
    ErrorCreateFlow,
    ErrorUpdateFlow,
    ErrorArchiveFlow,
    ErrorRestoreFlow,
    ErrorCreateCategory,
    ErrorUpdateCategory,
    ErrorArchiveCategory,
    ErrorCreateAlias,
    ErrorDeleteAlias,
    ErrorCreateVault,
    ErrorDeleteVault,
    ErrorSaveDefaults,
    ErrorMergeCategories,

    // Misc (4)
    MiscYes,
    MiscNo,
    MiscAll,
    MiscNone,

    // Section Labels (Capitalized for tab bar)
    SectionAccounts,
    SectionAnalytics,
    SectionSettings,

    // Transfer picker
    TransferPickerTitle,
    TransferPickerWallet,
    TransferPickerFlow,

    // UI Improvements - Stats
    StatsQuickActionRefresh,
    StatsQuickActionPeriod,
    StatsTrendExcellent,
    StatsTrendGood,
    StatsTrendStable,
    StatsTrendDeclining,
    StatsTrendCaution,
    StatsTrendRising,

    // UI Improvements - Accounts
    AccountsWelcomeTitle,
    AccountsWelcomeDesc,
    AccountsEnvelopesTitle,
    AccountsEnvelopesDesc,
    AccountsQuickCreate,
    AccountsCreateDetails,

    // UI Improvements - Transactions
    TransactionsFilterTypes,
    TransactionsSyntaxHelp,

    // Error Messages - API/Login
    ErrorInvalidCredentials,
    ErrorMembershipLastOwner,
    ErrorMembershipOwnerImmutable,
    ErrorMembershipOwnerRemoveForbidden,
    ErrorOperationForbidden,
    ErrorResourceNotFound,
    ErrorConflict,
    ErrorValidation,
    ErrorValidationAmbiguousVault,
    ErrorBadRequest,
    ErrorServerError,
    ErrorServerUnreachable,

    // State Messages
    StateSnapshotUnavailable,
    StateVaultUnavailable,
    StateUserUnavailable,
    StateNoWalletAvailable,
    StateUnallocatedMissing,
    StateCannotDetermineWalletTransfer,
    StateCannotDetermineFlowTransfer,

    // UI Labels
    UiVaultLabel,
    UiUserLabel,
    UiMainLabel,
    UiFetchingVaultData,
    UiNoData,
    UiOther,
    UiError,
    UiConnectionError,
    UiUnableToConnect,
    UiFailedToDeleteVault,
    UiTypeToSearchAllData,
    UiNoMatchingResults,

    // Dialog/Modal Labels
    DialogBulkCategorizeTitle,
    DialogBulkCategorizeConfirm,
    DialogBulkCategorizeCancel,
    DialogGroupByDate,
    DialogGroupByCategory,
    DialogGroupByWallet,
    DialogGroupByEnvelope,
    DialogCancel,

    // Prompts and Hints
    PromptEnterName,
    PromptEnterUsername,
    PromptEnterAlias,
    PromptEnterCap,
    PromptFillAllFields,
    PromptUseDedicatedTransferForm,
    PromptAtLeastTwoCategories,
    PromptSourceCategoryInvalid,
    PromptDestinationCategoryInvalid,
    PromptPressEnterToConfirm,
    PromptCheckConflicts,
    PromptNoShareableFlows,
    PromptNoMemberSelected,
    PromptNoAliasSelected,
    PromptNoCategorySelected,
    PromptEnterCategory,

    // Quick Add / Parsing Errors
    QuickAddEnterAmount,
    QuickAddAmountMissing,
    QuickAddAmountInvalid,
    QuickAddAmountMustBePositive,
    QuickAddTooManyCategories,
    QuickAddTooManyWallets,
    QuickAddTooManyEnvelopes,
    QuickAddSpecifyTwoWallets,
    QuickAddSpecifyTwoFlows,
    QuickAddWalletNotFound,
    QuickAddEnvelopeNotFound,
    QuickAddFlowNotFound,
    QuickAddWalletsMustBeDifferent,
    QuickAddFlowsMustBeDifferent,

    // Validation Errors - Additional
    ValidationNoTransactionSelected,
    ValidationNoWalletAvailable,
    ValidationNoFlowAvailable,
    ValidationNoElementAvailable,
    ValidationTransactionVoided,
    ValidationTransactionInvalid,
    ValidationTransferWalletInvalid,
    ValidationTransferFlowInvalid,
    ValidationWalletArchived,
    ValidationFlowArchived,
    ValidationSnapshotUnavailable,

    // Success Messages - Additional
    SuccessDeletedItem,
    SuccessDeletedMultiple,
    SuccessCategorizedTransactions,
    SuccessDeletedWallet,
    SuccessDeletedFlow,

    // UI Labels - Additional
    UiNoDetailAvailable,
    UiNoElement,
    UiNoRecentCategories,
    UiRecentCategories,
    UiTransactionDetail,
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
