//! Initial schema migration - creates all tables from scratch.
//!
//! This is a consolidated migration that replaces all previous migrations.
//! It creates the complete schema for Sparagne:
//!
//! - `users`: authentication
//! - `vaults`: budget containers owned by users
//! - `wallets`: physical money locations (cash, bank, card)
//! - `cash_flows`: logical budget buckets (vacations, emergency fund)
//! - `transactions`: financial operations with metadata
//! - `legs`: individual balance changes per transaction
//! - `vault_memberships`: multi-user vault access
//! - `flow_memberships`: multi-user flow access

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

// ─────────────────────────────────────────────────────────────────────────────
// Table identifiers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Iden)]
enum Users {
    Table,
    Username,
    Password,
    TelegramId,
    PairCode,
}

#[derive(Iden)]
enum Vaults {
    Table,
    Id,
    Name,
    UserId,
    Currency,
}

#[derive(Iden)]
enum Wallets {
    Table,
    Id,
    Name,
    Balance,
    Currency,
    Archived,
    VaultId,
}

#[derive(Iden)]
enum CashFlows {
    Table,
    Id,
    Name,
    SystemKind,
    Balance,
    MaxBalance,
    IncomeBalance,
    Currency,
    Archived,
    VaultId,
}

#[derive(Iden)]
enum Transactions {
    Table,
    Id,
    VaultId,
    Kind,
    OccurredAt,
    AmountMinor,
    Currency,
    Category,
    Note,
    CreatedBy,
    VoidedAt,
    VoidedBy,
    RefundedTransactionId,
    IdempotencyKey,
}

#[derive(Iden)]
enum Legs {
    Table,
    Id,
    TransactionId,
    TargetKind,
    TargetId,
    AmountMinor,
    Currency,
    AttributedUserId,
}

#[derive(Iden)]
enum VaultMemberships {
    Table,
    VaultId,
    UserId,
    Role,
}

#[derive(Iden)]
enum FlowMemberships {
    Table,
    FlowId,
    UserId,
    Role,
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ───────────────────────────────────────────────────────────────────
        // 1. Users
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Username)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Users::Password).string().not_null())
                    .col(ColumnDef::new(Users::TelegramId).string())
                    .col(ColumnDef::new(Users::PairCode).string())
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 2. Vaults
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Vaults::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Vaults::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Vaults::Name).string().not_null())
                    .col(ColumnDef::new(Vaults::UserId).string().not_null())
                    .col(
                        ColumnDef::new(Vaults::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vaults-user_id")
                            .from(Vaults::Table, Vaults::UserId)
                            .to(Users::Table, Users::Username),
                    )
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 3. Wallets
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Wallets::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Wallets::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Wallets::Name).string().not_null())
                    .col(ColumnDef::new(Wallets::Balance).big_integer().not_null())
                    .col(
                        ColumnDef::new(Wallets::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .col(ColumnDef::new(Wallets::Archived).boolean().not_null())
                    .col(ColumnDef::new(Wallets::VaultId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-wallets-vault_id")
                            .from(Wallets::Table, Wallets::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-wallets-vault_id-name-unique")
                    .table(Wallets::Table)
                    .col(Wallets::VaultId)
                    .col(Wallets::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 4. Cash Flows
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(CashFlows::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CashFlows::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CashFlows::Name).string().not_null())
                    .col(ColumnDef::new(CashFlows::SystemKind).string())
                    .col(ColumnDef::new(CashFlows::Balance).big_integer().not_null())
                    .col(ColumnDef::new(CashFlows::MaxBalance).big_integer())
                    .col(ColumnDef::new(CashFlows::IncomeBalance).big_integer())
                    .col(
                        ColumnDef::new(CashFlows::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .col(ColumnDef::new(CashFlows::Archived).boolean().not_null())
                    .col(ColumnDef::new(CashFlows::VaultId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-cash_flows-vault_id")
                            .from(CashFlows::Table, CashFlows::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-cash_flows-vault_id-name-unique")
                    .table(CashFlows::Table)
                    .col(CashFlows::VaultId)
                    .col(CashFlows::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 5. Transactions
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Transactions::VaultId).string().not_null())
                    .col(ColumnDef::new(Transactions::Kind).string().not_null())
                    .col(
                        ColumnDef::new(Transactions::OccurredAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Transactions::AmountMinor)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Transactions::Currency).string().not_null())
                    .col(ColumnDef::new(Transactions::Category).string())
                    .col(ColumnDef::new(Transactions::Note).string())
                    .col(ColumnDef::new(Transactions::CreatedBy).string().not_null())
                    .col(ColumnDef::new(Transactions::VoidedAt).timestamp())
                    .col(ColumnDef::new(Transactions::VoidedBy).string())
                    .col(ColumnDef::new(Transactions::RefundedTransactionId).string())
                    .col(ColumnDef::new(Transactions::IdempotencyKey).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-vault_id")
                            .from(Transactions::Table, Transactions::VaultId)
                            .to(Vaults::Table, Vaults::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-vault_id-occurred_at")
                    .table(Transactions::Table)
                    .col(Transactions::VaultId)
                    .col(Transactions::OccurredAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-idempotency_key")
                    .table(Transactions::Table)
                    .col(Transactions::VaultId)
                    .col(Transactions::IdempotencyKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-created_by")
                    .table(Transactions::Table)
                    .col(Transactions::CreatedBy)
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 6. Legs
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Legs::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Legs::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Legs::TransactionId).string().not_null())
                    .col(ColumnDef::new(Legs::TargetKind).string().not_null())
                    .col(ColumnDef::new(Legs::TargetId).string().not_null())
                    .col(ColumnDef::new(Legs::AmountMinor).big_integer().not_null())
                    .col(ColumnDef::new(Legs::Currency).string().not_null())
                    .col(ColumnDef::new(Legs::AttributedUserId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-legs-transaction_id")
                            .from(Legs::Table, Legs::TransactionId)
                            .to(Transactions::Table, Transactions::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-legs-transaction_id")
                    .table(Legs::Table)
                    .col(Legs::TransactionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-legs-target")
                    .table(Legs::Table)
                    .col(Legs::TargetKind)
                    .col(Legs::TargetId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-legs-target_id")
                    .table(Legs::Table)
                    .col(Legs::TargetId)
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 7. Vault Memberships
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(VaultMemberships::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(VaultMemberships::VaultId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(VaultMemberships::UserId).string().not_null())
                    .col(ColumnDef::new(VaultMemberships::Role).string().not_null())
                    .primary_key(
                        Index::create()
                            .col(VaultMemberships::VaultId)
                            .col(VaultMemberships::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vault_memberships-vault_id")
                            .from(VaultMemberships::Table, VaultMemberships::VaultId)
                            .to(Vaults::Table, Vaults::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vault_memberships-user_id")
                            .from(VaultMemberships::Table, VaultMemberships::UserId)
                            .to(Users::Table, Users::Username)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-vault_memberships-user_id")
                    .table(VaultMemberships::Table)
                    .col(VaultMemberships::UserId)
                    .to_owned(),
            )
            .await?;

        // ───────────────────────────────────────────────────────────────────
        // 8. Flow Memberships
        // ───────────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(FlowMemberships::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(FlowMemberships::FlowId).string().not_null())
                    .col(ColumnDef::new(FlowMemberships::UserId).string().not_null())
                    .col(ColumnDef::new(FlowMemberships::Role).string().not_null())
                    .primary_key(
                        Index::create()
                            .col(FlowMemberships::FlowId)
                            .col(FlowMemberships::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-flow_memberships-flow_id")
                            .from(FlowMemberships::Table, FlowMemberships::FlowId)
                            .to(CashFlows::Table, CashFlows::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-flow_memberships-user_id")
                            .from(FlowMemberships::Table, FlowMemberships::UserId)
                            .to(Users::Table, Users::Username)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-flow_memberships-user_id")
                    .table(FlowMemberships::Table)
                    .col(FlowMemberships::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop in reverse order of creation (respecting FK dependencies)
        manager
            .drop_table(Table::drop().table(FlowMemberships::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(VaultMemberships::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Legs::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(CashFlows::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Wallets::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Vaults::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;
        Ok(())
    }
}
