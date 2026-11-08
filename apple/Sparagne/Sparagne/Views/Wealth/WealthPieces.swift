import SwiftUI
import SparagneCore

// The small pieces the Patrimonio's inspector and sheets are drawn with,
// over FormKit's: a fact on a dashed rule, a typed figure with its unit, the
// three Pagamento choices, the preview's figures and a checkbox.

/// A sheet of the Patrimonio, asked of the window, which presents it
/// (`MainWindow.SheetKind`): the tab cannot reach the window's sheet
/// otherwise.
enum LoanSheetRequest: Hashable, Sendable {
    case edit(Uuid)
    case addEvent(Uuid)
    case schedule(Uuid)
    case createRecurring(Uuid)

    @MainActor
    func post() {
        NotificationCenter.default.post(name: .loanSheet, object: self)
    }
}

/// A label and its value on a dashed rule, read as one: the inspector's
/// Termini and A oggi.
struct WealthFact: View {
    let label: String
    let value: String
    var tint: Color = Ink.text

    init(_ label: String, _ value: String, tint: Color = Ink.text) {
        self.label = label
        self.value = value
        self.tint = tint
    }

    var body: some View {
        HStack {
            Text(label).foregroundStyle(Ink.text2)
            Spacer(minLength: 8)
            Text(value).fontWeight(.semibold).foregroundStyle(tint).lineLimit(1)
        }
        .font(Face.row)
        .frame(minHeight: 22)
        .overlay(alignment: .bottom) { DashedRule() }
        .accessibilityElement(children: .combine)
    }
}

/// A figure typed in a sheet, right-aligned in its box with its unit after
/// it in `text3`: `200.000,00 €`, `3,000 %`. The text is the draft's, as
/// typed; the draft reads it.
struct UnitField: View {
    let label: String
    @Binding var text: String
    var unit: String?
    var prompt: String?

    @FocusState private var focused: Bool
    @Environment(\.formMetrics) private var metrics

    var body: some View {
        FormBox(focused: metrics.showsFocus && focused) {
            TextField(label, text: $text, prompt: Text(verbatim: prompt ?? "").foregroundStyle(Ink.text3))
                .textFieldStyle(.plain)
                .labelsHidden()
                .multilineTextAlignment(.trailing)
                .focused($focused)
                .accessibilityLabel(label)
            if let unit {
                Text(verbatim: unit)
                    .foregroundStyle(Ink.text3)
                    .accessibilityHidden(true)
            }
        }
    }
}

/// A day typed as a `Date`, as the loan drafts keep it, in the app's
/// `DayField`, which works on the core's days.
struct LoanDayField: View {
    let label: String
    @Binding var date: Date

    var body: some View {
        DayField(
            label: label,
            day: Binding(
                get: { CoreDate.day(date) },
                set: { day in if let picked = CoreDate.localDay(day) { date = picked } }
            )
        )
    }
}

/// One of the Pagamento choices (`.opt`): a title and a line under it,
/// the chosen one in the accent's tint.
struct ChoiceCard: View {
    let title: String
    let detail: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(Face.ui(12, .semibold))
                    .foregroundStyle(selected ? Ink.accent : Ink.text)
                Text(detail)
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
                    .lineLimit(1)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(selected ? Ink.accent.opacity(0.14) : .clear, in: RoundedRectangle(cornerRadius: 7))
            .overlay(
                RoundedRectangle(cornerRadius: 7)
                    .strokeBorder(selected ? Ink.accent.opacity(0.4) : Ink.line2, lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityHint(detail)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }
}

/// One figure of a sheet's preview (`.prev`): a label, the figure, and an
/// optional line under it.
struct PreviewFigure: Identifiable {
    let label: String
    let value: String
    var caption: String?
    var tint: Color = Ink.text

    var id: String { label }
}

/// The preview's box: its figures in a grid on the chrome's ground.
struct PreviewBlock: View {
    let figures: [PreviewFigure]
    var columns = 4

    var body: some View {
        LazyVGrid(
            columns: Array(repeating: GridItem(.flexible(minimum: 110), spacing: 16, alignment: .topLeading), count: columns),
            alignment: .leading,
            spacing: 10
        ) {
            ForEach(figures) { figure in
                VStack(alignment: .leading, spacing: 1) {
                    Text(figure.label)
                        .font(Face.ui(11))
                        .foregroundStyle(Ink.text3)
                    Text(figure.value)
                        .font(Face.ui(15, .semibold))
                        .foregroundStyle(figure.tint)
                    if let caption = figure.caption {
                        Text(caption)
                            .font(Face.ui(11))
                            .foregroundStyle(Ink.text3)
                    }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(AccessibilityText.card(title: figure.label, value: figure.value, caption: figure.caption))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.bg, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Ink.line, lineWidth: 1))
    }
}

/// The canvas's 14-point checkbox and its words: the accent with a dark
/// tick when on. Read as a toggle.
struct WealthCheckbox: View {
    let label: String
    @Binding var isOn: Bool

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            HStack(spacing: 8) {
                ZStack {
                    RoundedRectangle(cornerRadius: 4)
                        .fill(isOn ? Ink.accent : .clear)
                    RoundedRectangle(cornerRadius: 4)
                        .strokeBorder(isOn ? Ink.accent : Ink.line2, lineWidth: 1)
                    if isOn {
                        Image(systemName: "checkmark")
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(Ink.onAccent)
                    }
                }
                .frame(width: 14, height: 14)
                Text(label)
                    .font(Face.ui(13, .semibold))
                    .foregroundStyle(Ink.text)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(isOn ? String(localized: "Enabled") : String(localized: "Disabled"))
    }
}

/// The wallet, flow, category and person a loan's money is recorded with,
/// as rows of a sheet: the template made with a loan, or an early
/// repayment's expense. `showsOwner: false` leaves the person to the author.
struct LoanTargetRows: View {
    let store: AppStore
    @Binding var target: LoanLedgerTarget
    let completion: CategoryCompletionModel
    var showsOwner = true

    var body: some View {
        let names = store.names
        FormRow(String(localized: "Wallet")) {
            FormMenu(
                label: String(localized: "Wallet"),
                value: target.walletId.map { names.wallet($0) ?? TransactionRow.placeholder }
                    ?? String(localized: "Any wallet")
            ) {
                if store.wallets.count <= 1 {
                    Button(String(localized: "Any wallet")) { target.walletId = nil }
                    Divider()
                }
                ForEach(store.wallets, id: \.id) { wallet in
                    Button(wallet.name) { target.walletId = wallet.id }
                }
            }
            .frame(width: 240)
        }
        FormRow(String(localized: "Flow")) {
            FormMenu(
                label: String(localized: "Flow"),
                value: target.flowId.map { names.flow($0) ?? TransactionRow.placeholder } ?? NameBook.unallocatedLabel
            ) {
                Button(NameBook.unallocatedLabel) { target.flowId = nil }
                Divider()
                ForEach(store.flows.filter { !$0.isUnallocated }, id: \.id) { flow in
                    Button(flow.name) { target.flowId = flow.id }
                }
            }
            .frame(width: 240)
        }
        FormRow(String(localized: "Category")) {
            CategoryFormField(label: String(localized: "Category"), text: $target.category, store: store, completion: completion)
                .frame(width: 240)
        }
        if showsOwner {
            FormRow(String(localized: "Person")) {
                FormPicker(
                    label: String(localized: "Person"),
                    selection: Binding(
                        get: { target.owner ?? store.currentAuthor },
                        set: { target.owner = $0 }
                    ),
                    options: AppStore.distinct(store.assignablePeople + [target.owner ?? store.currentAuthor]),
                    title: { $0 }
                )
                .frame(width: 240)
            }
        }
    }

    /// Where a new loan's money goes unless the user says otherwise: the
    /// wallet the ledger would pick, the flow named Casa when there is one,
    /// the category Mutuo, the author.
    static func defaults(_ store: AppStore) -> LoanLedgerTarget {
        let home = store.flows.first { !$0.isUnallocated && $0.name.localizedCaseInsensitiveCompare("Casa") == .orderedSame }
        return LoanLedgerTarget(
            walletId: store.defaultWalletId,
            flowId: home?.id,
            category: String(localized: "loan.category", defaultValue: "Mortgage"),
            owner: store.currentAuthor
        )
    }
}
