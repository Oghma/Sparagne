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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKey {
    // Sections
    SectionHome,
    SectionTransactions,
    SectionWallets,
    SectionFlows,
    SectionCategories,
    SectionMembers,
    SectionVault,
    SectionPreferences,
    SectionAccounts,
    SectionAnalytics,
    SectionSettings,

    // Actions
    ActionSave,

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
    HelpRefreshData,
    HelpSwitchView,
    HelpSwitchPanel,
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
    HelpUnshareFlow,

    // Status bar
    StatusOnline,
    StatusOffline,

    // Success Messages
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
    SuccessDeletedItem,
    SuccessDeletedMultiple,
    SuccessCategorizedTransactions,
    SuccessDeletedWallet,
    SuccessDeletedFlow,
    SuccessFlowUnshared,

    // Error Messages
    ErrorSaving,
    ErrorUpdating,
    ErrorVoiding,
    ErrorRepeating,
    ErrorConnection,
    ErrorTransferWallet,
    ErrorTransferFlow,
    ErrorCreateWallet,
    ErrorUpdateWallet,
    ErrorArchiveWallet,
    ErrorRestoreWallet,
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
    UiOther,
    UiError,
    UiFailedToDeleteVault,
    UiNoDetailAvailable,
    UiNoElement,
    UiNoRecentCategories,
    UiRecentCategories,

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

    // Validation Errors
    ValidationRequired,
    ValidationAmountRequired,
    ValidationAmountInvalid,
    ValidationAmountPositive,
    ValidationDateRequired,
    ValidationDateInvalid,
    ValidationDateInvalidTimezone,
    ValidationLengthMin,
    ValidationLengthMax,
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

    // Home screen stat cards
    HomeCardIncome,
    HomeCardExpenses,
    HomeCardThisMonth,

    // Dialog titles and messages
    DialogUnsavedChangesTitle,
    DialogUnsavedChangesMessage,
    DialogSave,
    DialogDiscard,
    DialogDelete,
    DialogDeleteVaultTitle,
    DialogDeleteVaultWarning,
    DialogDeleteTransactionsTitle,
    DialogDeleteTransactionTitle,
    DialogDeleteUndoHint,
    DialogDeleteWalletTitle,
    DialogDeleteWalletHint,
    DialogDeleteFlowTitle,
    DialogDeleteFlowHint,
    DialogDeleteCategoryTitle,
    DialogDeleteCategoryHint,
    DialogThisVault,
    DialogTransaction,
    DialogConnectionErrorTitle,
    DialogConnectionErrorMessage,
    DialogUnshareFlowTitle,
    DialogUnshareFlowMessage,

    // Form field labels
    FormAmount,
    FormWallet,
    FormFlow,
    FormCategory,
    FormNote,
    FormWhen,

    // Form field helpers
    FormHelperAmount,
    FormHelperWallet,
    FormHelperFlow,
    FormHelperCategory,
    FormHelperNote,
    FormHelperWhen,

    // Form titles
    FormEditIncome,
    FormNewIncome,
    FormEditExpense,
    FormNewExpense,
    FormEditRefund,
    FormNewRefund,
    FormEditTransaction,
    FormNewTransaction,

    // Form footer hints
    FormHintSave,
    FormHintCancel,
    FormHintNextField,
    FormHintCycleChoices,

    // Transaction list / common labels
    TxnScopeAll,
    TxnUncategorized,
    TxnNoWallet,
    TxnNoEnvelope,
    TxnRecentsPrefix,
    TxnRecentsCategories,
    TxnRecentsWallet,
    TxnRecentsFlow,

    // Quick add
    QuickAddTitle,
    QuickAddPlaceholder,
    QuickAddToday,
    QuickAddSyntaxHint,
    QuickAddSyntaxShort,
    QuickAddExamples,
    QuickAddEnvelopeSuggestions,
    QuickAddCycle,

    // Stats labels
    StatsTitle,
    StatsNoData,
    StatsRefreshHint,
    StatsMonthSummary,
    StatsMoM,
    StatsInc,
    StatsExp,
    StatsNa,
    StatsIncome,
    StatsExpenses,
    StatsExpenseOverIncome,
    StatsNoIncomeToCompare,
    StatsNet,
    StatsBalance,
    StatsCategoryBreakdown,
    StatsNoCategoryData,
    StatsDistribution,
    StatsBalanceTrend,
    StatsMonthlyTrend,
    StatsMonthlyTrendNoData,
    StatsFinancialTrends,
    StatsNetSavings,
    StatsTotalIncome,
    StatsTotalExpenses,
    StatsNetBalance,
    StatsThisMonth,
    StatsExpenseTrend,
    StatsNoExpenseData,
    StatsTabCashFlow,
    StatsTabSpending,
    StatsTabNetWorth,

    // Category screen labels
    CatTitle,
    CatNoCategoriesYet,
    CatCreateFirst,
    CatRenameTitle,
    CatCurrentLabel,
    CatNewNameLabel,
    CatAliasesFor,
    CatNoAliasesForCategory,
    CatTypeToAddAlias,
    CatNewAlias,
    CatSwitchFocus,
    CatAliasesTitle,
    CatNoCategorySelected,
    CatCategoryLabel,
    CatPressToLoadAliases,
    CatNoAliases,
    CatMore,
    CatNewCategoryTitle,
    CatMergeCategoriesTitle,
    CatMergeLabel,
    CatMergePreviewOkMerge,
    CatMergePreviewOkConfirm,
    CatMergeConflicts,
    CatMergeSelectTarget,
    CatMergePreviewAction,
    CatConflictSameCategory,
    CatConflictSourceSystem,
    CatConflictTargetArchived,
    CatConflictAliasConflict,
    CatConflictNameConflict,
    CatConflictGeneric,

    // Category list badges
    CatBadgeSystem,
    CatBadgeArchived,
    CatBadgeFrom,
    CatBadgeTo,

    // Category form hints
    CatHintCreate,
    CatHintRename,
    CatHintAliases,
    CatHintMerge,

    // Transaction header / grouping
    TxnGroupDate,
    TxnGroupCategory,
    TxnGroupWallet,
    TxnGroupEnvelope,
    TxnHeaderFiltersOff,
    TxnHeaderSearch,

    // Transaction detail
    TxnDetailTitle,
    TxnDetailKind,
    TxnDetailVoided,
    TxnDetailVoidedYes,
    TxnDetailVoidedNo,
    TxnDetailWhen,
    TxnDetailAmount,
    TxnDetailCategory,
    TxnDetailNote,
    TxnDetailLegsTitle,
    TxnDetailLegWallet,
    TxnDetailLegFlow,

    // Vault view / defaults
    VaultDefaultName,
    VaultQuickDefaults,
    VaultDefaultWallet,
    VaultDefaultFlow,
    VaultIdLabel,
    VaultCurrencyLabel,

    // Loading & empty states
    LoadingGeneric,
    LoadingVaultData,
    SearchLabel,
    SearchNoResults,
    SearchClearHint,
    SearchClearShort,

    // Date labels
    DateToday,
    DateYesterday,

    // Entity list shared
    EntityArchivedOn,
    EntityBadgeArchived,
    EntityBadgeDefault,

    // Wallet screen
    WalletTitle,
    WalletDetailTitle,
    WalletNotFound,
    WalletSelectPrompt,
    WalletNoTransactions,
    WalletWelcomeTitle,
    WalletWelcomeDesc1,
    WalletWelcomeDesc2,
    WalletHintQuickCreate,
    WalletHintCreateDetails,
    FormTitleRenameWallet,
    FormTitleNewWallet,

    // Flow screen
    FlowTitle,
    FlowDetailTitle,
    FlowNotFound,
    FlowSelectPrompt,
    FlowNoTransactions,
    FlowWelcomeTitle,
    FlowWelcomeDesc1,
    FlowWelcomeDesc2,
    FlowHintQuickCreate,
    FlowHintCreateCap,
    FormTitleRenameFlow,
    FormTitleNewFlow,

    // Home screen
    HomeActivityFeed,
    HomeNetWorth,
    HomeQuickBalances,
    HomeNoDataYet,
    HomeAddFirstTxn,
    HomeWallets,
    HomeBudgets,
    HomeNoActivityYet,

    // Settings
    SettingsCardTitle,
    PreferencesTitle,

    // Members
    MembersVaultTitle,
    MembersEditTitle,
    MembersAddTitle,

    // Vault
    VaultCreateTitle,

    // Transfers & pickers
    TransferWalletTitle,
    TransferFlowTitle,
    TransferEditWalletTitle,
    TransferEditFlowTitle,
    TransferTypeTitle,
    TransferFrom,
    TransferTo,
    TransferAvailable,
    TransferBadgeFrom,
    TransferBadgeTo,
    TransferFormHints,

    // Filters
    FilterTitle,
    FilterFrom,
    FilterTo,
    FilterTransactionTypes,
    FilterToggleHint,
    FilterKindIncome,
    FilterKindExpense,
    FilterKindRefund,
    FilterKindWalletTransfer,
    FilterKindFlowTransfer,

    // Pickers
    PickerAllWallets,
    PickerAllFlows,
    PickerSelectWallet,
    PickerSelectFlow,
    PickerBadgeUnallocated,
    PickerSuffixArchived,

    // Grouping dialog
    GroupingTitle,
    GroupingDate,
    GroupingCategory,
    GroupingWallet,
    GroupingEnvelope,
    GroupingCurrent,

    // Transactions list
    TxnNoTransactionsYet,
    TxnAddOneHint,
    TxnSearchEditClearHint,

    // Scope labels (transaction common)
    ScopeFlowLabel,
    ScopeFlowUnknown,
    ScopeWalletLabel,
    ScopeWalletUnknown,

    // Shell / Status
    ShellVaultLabel,
    ShellUserLabel,
    ShellVaultFallback,

    // Error dialog
    ErrorTechnicalDetails,

    // Flow form
    FormLabelAllowNegative,
    FlowBadgeAllowNegative,
    FlowBadgeCapped,
    FlowBadgeShared,
    FlowBadgeSharedFrom,
    FlowBadgeSharing,
    FlowDetailOwner,
    FlowDetailYourAccess,
    FlowDetailSharedWith,
    FlowActionUnshare,
    FlowActionManageSharing,

    // Recurring
    RecurringTitle,
    RecurringEmpty,
    RecurringPending,
    RecurringFreqDaily,
    RecurringFreqWeekly,
    RecurringFreqMonthly,
    RecurringFreqYearly,
    RecurringFormTitle,
    RecurringFormEditTitle,
    RecurringFormKind,
    RecurringFormAmount,
    RecurringFormFrequency,
    RecurringFormDay,
    RecurringFormStartDate,
    RecurringFormEndDate,
    RecurringKindIncome,
    RecurringKindExpense,
    RecurringEnabled,
    RecurringDisabled,
    RecurringCreated,
    RecurringUpdated,
    RecurringArchived,
    RecurringExecuted,

    // General UI
    UiNone,
    UiUndoApplied,
    UiYes,
    UiNo,
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
