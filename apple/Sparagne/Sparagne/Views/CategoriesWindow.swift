import SwiftUI
import SparagneCore

/// The `Window` scene behind the "Categories…" (⌘⇧C) menu item
/// (`SparagneApp`): create, rename, archive/restore categories and their
/// aliases, and merge duplicates with a preview step
/// (docs/v2/DISTILLATO_V1.md §2.1).
struct CategoriesWindowView: View {
    let store: AppStore
    @State private var selection: Uuid?
    @State private var newName = ""
    @State private var sheet: Sheet?
    /// Errors from this window's own commands. Kept separate from
    /// `store.presentedError`: that one is shared with the main window's
    /// alert, and both windows can be open at once.
    @State private var localError: AppError?

    private enum Sheet: Identifiable {
        case rename(CategoryView)
        case alias(CategoryView)
        case merge(CategoryView)

        var id: String {
            switch self {
            case .rename(let category): "rename-\(category.id)"
            case .alias(let category): "alias-\(category.id)"
            case .merge(let category): "merge-\(category.id)"
            }
        }
    }

    private var suggestions: [CategoryView] { store.similarCategories(name: newName) }

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                ForEach(store.windowCategories, id: \.id) { category in
                    CategoryRow(category: category, aliasCount: aliases(for: category.id).count)
                        .tag(category.id)
                        .contextMenu {
                            Button(String(localized: "Rename…")) { sheet = .rename(category) }
                            Button(String(localized: "Manage Aliases…")) { sheet = .alias(category) }
                            if !category.isSystem {
                                Button(String(localized: "Merge Into…")) { sheet = .merge(category) }
                                Divider()
                                if category.archived {
                                    Button(String(localized: "Restore")) { run { store.restoreCategory(category.id) } }
                                } else {
                                    Button(String(localized: "Archive"), role: .destructive) {
                                        run { store.archiveCategory(category.id) }
                                    }
                                }
                            }
                        }
                }
            }
            .navigationTitle(String(localized: "Categories"))
        } detail: {
            newCategoryForm
        }
        .frame(minWidth: 520, minHeight: 360)
        .onAppear { store.loadCategoryManagement() }
        .onChange(of: store.currentVault?.id) { _, _ in store.loadCategoryManagement() }
        .sheet(item: $sheet) { kind in
            switch kind {
            case .rename(let category):
                RenameSheet(title: String(localized: "Rename Category"), name: category.name) { name in
                    run { store.renameCategory(category.id, name: name) }
                }
            case .alias(let category):
                AliasSheet(store: store, category: category, run: run)
            case .merge(let category):
                MergeCategorySheet(
                    store: store,
                    source: category,
                    targets: store.windowCategories.filter { $0.id != category.id && !$0.archived },
                    run: run
                )
            }
        }
        .alert(
            localError.map { ErrorMessages.summary(for: $0.code) } ?? String(localized: "Something went wrong"),
            isPresented: Binding(get: { localError != nil }, set: { if !$0 { localError = nil } }),
            presenting: localError
        ) { _ in
            Button(String(localized: "OK"), role: .cancel) { localError = nil }
        } message: { error in
            Text(error.message)
        }
    }

    private func aliases(for categoryId: Uuid) -> [AliasView] {
        store.categoryAliases.filter { $0.categoryId == categoryId }
    }

    /// Runs a store command, then drains `store.presentedError` into this
    /// window's own alert so the main window (if open) does not also show
    /// it (`store.presentedError` is shared `AppStore` state).
    private func run(_ work: () -> Void) {
        work()
        if let error = store.presentedError {
            store.presentedError = nil
            localError = error
        }
    }

    @ViewBuilder
    private var newCategoryForm: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(String(localized: "New Category")).font(.headline)
            TextField(String(localized: "Name"), text: $newName)
                .onSubmit(createCategory)
            if !suggestions.isEmpty {
                Text(String(localized: "Similar:") + " " + suggestions.map(\.name).joined(separator: ", "))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Button(String(localized: "Create"), action: createCategory)
                .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func createCategory() {
        let name = newName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        run { store.createCategory(name: name) }
        newName = ""
    }
}

private struct CategoryRow: View {
    let category: CategoryView
    let aliasCount: Int

    var body: some View {
        HStack {
            Text(category.name)
                .strikethrough(category.archived)
            if category.isSystem {
                Text(String(localized: "System"))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if aliasCount > 0 {
                Text("\(aliasCount)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// Add and remove aliases for one category.
private struct AliasSheet: View {
    let store: AppStore
    let category: CategoryView
    let run: (() -> Void) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var newAlias = ""

    private var aliases: [AliasView] {
        store.categoryAliases.filter { $0.categoryId == category.id }.sorted { $0.alias < $1.alias }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Aliases for") + " " + category.name).font(.headline)
            List {
                ForEach(aliases, id: \.id) { alias in
                    HStack {
                        Text(alias.alias)
                        Spacer()
                        Button {
                            run { store.removeAlias(categoryId: category.id, alias: alias.alias) }
                        } label: {
                            Image(systemName: "minus.circle")
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
            .frame(minHeight: 120)
            HStack {
                TextField(String(localized: "New alias"), text: $newAlias)
                    .onSubmit(addAlias)
                Button(String(localized: "Add"), action: addAlias)
                    .disabled(newAlias.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            HStack {
                Spacer()
                Button(String(localized: "Done")) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 360)
    }

    private func addAlias() {
        let alias = newAlias.trimmingCharacters(in: .whitespaces)
        guard !alias.isEmpty else { return }
        run { store.addAlias(categoryId: category.id, alias: alias) }
        newAlias = ""
    }
}

/// Merges `source` into a chosen target, previewing conflicts first and
/// disabling Merge when the preview is not `ok`
/// (`Core::preview_merge`, `docs/v2/ARCH.md` §4).
private struct MergeCategorySheet: View {
    let store: AppStore
    let source: CategoryView
    let targets: [CategoryView]
    let run: (() -> Void) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var targetId: Uuid?
    @State private var preview: MergePreview?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Merge") + " “\(source.name)” " + String(localized: "into")).font(.headline)
            Picker(String(localized: "Target category"), selection: $targetId) {
                Text(String(localized: "Choose…")).tag(Optional<Uuid>.none)
                ForEach(targets, id: \.id) { target in
                    Text(target.name).tag(Optional(target.id))
                }
            }
            .onChange(of: targetId) { _, newValue in
                preview = newValue.flatMap { store.previewCategoryMerge(sourceId: source.id, targetId: $0) }
            }
            if let preview, !preview.conflicts.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(preview.conflicts.enumerated()), id: \.offset) { _, conflict in
                        Label(conflictText(conflict), systemImage: "exclamationmark.triangle")
                            .foregroundStyle(Ink.accent)
                            .font(.caption)
                    }
                }
            }
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                Button(String(localized: "Merge")) {
                    guard let targetId else { return }
                    run { store.mergeCategory(sourceId: source.id, targetId: targetId) }
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(targetId == nil || preview?.ok == false)
            }
        }
        .padding(20)
        .frame(width: 380)
    }

    private func conflictText(_ conflict: MergeConflict) -> String {
        switch conflict.kind {
        case .sameCategory: String(localized: "Source and target are the same category.")
        case .sourceSystem: String(localized: "System categories cannot be merged.")
        case .targetArchived: "“" + conflict.value + "” " + String(localized: "is archived.")
        }
    }
}
