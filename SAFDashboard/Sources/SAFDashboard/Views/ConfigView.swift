import SwiftUI

/// Edits the bot's safe-to-change settings. The bot serves its config with all
/// secrets stripped; we render the scalar SAFE fields, diff against what was
/// loaded, and PATCH only the keys the user actually changed. Mirrors the web
/// dashboard's Config view. Most changes apply on the next bot restart.
struct ConfigView: View {
    @EnvironmentObject var store: AppStore

    /// The config as last loaded from the bot, used to compute the save diff.
    @State private var original: [String: JSONValue] = [:]
    @State private var loading = false
    @State private var saving = false
    @State private var loaded = false
    @State private var loadError: String?

    // Editable scalar SAFE fields. Booleans/numbers/strings only — nested
    // objects (skip, doNotBuy, branding, …) are preserved untouched and not
    // sent in the patch.
    @State private var defaultIgn = ""
    @State private var startDefaultOnly = false
    @State private var useCookie = false
    @State private var autoCookie = ""
    @State private var angryCoopPrevention = false
    @State private var relist = false
    @State private var pingOnUpdate = false
    @State private var delay = ""
    @State private var buyingReadyDelay = ""
    @State private var waittime = ""
    @State private var percentOfTarget = ""
    @State private var listHours = ""
    @State private var clickDelay = ""
    @State private var bedSpam = false
    @State private var blockUselessMessages = false
    @State private var roundTo = ""
    @State private var visitFriend = false
    @State private var webhookFormat = ""
    // skip.* thresholds (a nested object): edited here, resent whole on save so
    // the other skip fields are preserved. These also set the Cofl flip safety
    // floor on the bot.
    @State private var skipMinProfit = ""
    @State private var skipMinPrice = ""
    @State private var skipProfitPercentage = ""

    var body: some View {
        Page(spacing: 18) {
            PageHeader("Config", subtitle: "Edit the bot's safe-to-change settings") {
                HStack(spacing: 10) {
                    GhostButton(title: "Reload", systemImage: "arrow.clockwise") { Task { await load() } }
                    PrimaryButton(title: saving ? "Saving…" : "Save", systemImage: "checkmark") {
                        Task { await save() }
                    }
                }
            }

            applyNote

            if let loadError {
                errorBanner(loadError)
            } else if loading && !loaded {
                EmptyState(icon: "hourglass", text: "Loading config…").frame(height: 240)
            } else if !loaded {
                EmptyState(icon: "slider.horizontal.3", text: store.reachable ? "No config loaded." : "Connect to the bot to edit its config.").frame(height: 240)
            } else {
                behaviorCard
                thresholdsCard
                timingCard
                listingCard
                notificationsCard
            }
        }
        .task {
            if !loaded && !loading { await load() }
        }
    }

    // MARK: Sections

    private var applyNote: some View {
        HStack(spacing: 10) {
            Image(systemName: "info.circle.fill").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.accent)
            Text("Most changes apply the next time the bot restarts. Secret fields (tokens, sessions, webhooks) are not editable here.")
                .font(.rounded(11.5, .medium)).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 10).fill(Theme.accent.opacity(0.08)))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.accent.opacity(0.18), lineWidth: 1))
    }

    private var behaviorCard: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Behavior", systemImage: "gearshape.2.fill")
                HStack(spacing: 14) {
                    FormField(label: "Default IGN", systemImage: "person.fill", hint: "Account started by default") {
                        TextField("ign", text: $defaultIgn)
                    }
                    FormField(label: "Auto-cookie", systemImage: "clock.arrow.circlepath", hint: "e.g. 2h, or blank to disable") {
                        TextField("2h", text: $autoCookie)
                    }
                }
                toggleRow("Start default only", "Only start the default account on launch.", $startDefaultOnly, icon: "person.crop.circle")
                toggleRow("Use cookie", "Keep the booster cookie active.", $useCookie, icon: "birthday.cake.fill")
                toggleRow("Angry-coop prevention", "Back off when a co-op partner is active.", $angryCoopPrevention, icon: "person.2.slash")
                toggleRow("Visit friend", "Visit a friend's island when needed.", $visitFriend, icon: "figure.walk")
            }
        }
    }

    private var thresholdsCard: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Flip thresholds", systemImage: "line.3.horizontal.decrease.circle.fill")
                HStack(spacing: 14) {
                    FormField(label: "Min profit", systemImage: "dollarsign.circle", hint: "Skip flips below this profit (e.g. 10m). Also sets the bot's Cofl safety floor.") {
                        TextField("100k", text: $skipMinProfit)
                    }
                    FormField(label: "Min profit %", systemImage: "percent", hint: "Skip flips below this margin.") {
                        TextField("5", text: $skipProfitPercentage)
                    }
                    FormField(label: "Min price", systemImage: "tag", hint: "Skip items cheaper than this.") {
                        TextField("50k", text: $skipMinPrice)
                    }
                }
            }
        }
    }

    private var timingCard: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Timing", systemImage: "timer")
                HStack(spacing: 14) {
                    numberField("Delay (ms)", "300", $delay, icon: "hourglass")
                    numberField("Buying-ready delay (ms)", "0", $buyingReadyDelay, icon: "cart")
                    numberField("Wait time (ms)", "0", $waittime, icon: "clock")
                }
                HStack(spacing: 14) {
                    numberField("Click delay (ms)", "0", $clickDelay, icon: "cursorarrow.click")
                    numberField("Round to", "0", $roundTo, icon: "number")
                    numberField("List hours", "0", $listHours, icon: "calendar")
                }
            }
        }
    }

    private var listingCard: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Listing", systemImage: "tag.fill")
                HStack(spacing: 14) {
                    numberField("% of target", "100", $percentOfTarget, icon: "percent")
                    Spacer()
                }
                toggleRow("Relist", "Automatically relist items that don't sell.", $relist, icon: "arrow.triangle.2.circlepath")
                toggleRow("Bed spam", "Use bed-spam micro-listing.", $bedSpam, icon: "bed.double.fill")
                toggleRow("Block useless messages", "Suppress noisy in-game chat.", $blockUselessMessages, icon: "speaker.slash.fill")
            }
        }
    }

    private var notificationsCard: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Notifications", systemImage: "bell.fill")
                FormField(label: "Webhook format", systemImage: "text.alignleft", hint: "Message template for webhook posts (the webhook URL itself stays secret).") {
                    TextField("format", text: $webhookFormat)
                }
                toggleRow("Ping on update", "Mention on each posted update.", $pingOnUpdate, icon: "at")
            }
        }
    }

    // MARK: Field helpers

    private func toggleRow(_ title: String, _ subtitle: String, _ binding: Binding<Bool>, icon: String) -> some View {
        HStack(spacing: 14) {
            Image(systemName: icon).font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.accent)
                .frame(width: 30, height: 30).background(Circle().fill(Theme.accent.opacity(0.15)))
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.rounded(13, .semibold)).foregroundStyle(Theme.textPrimary)
                Text(subtitle).font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary)
            }
            Spacer()
            Toggle("", isOn: binding).labelsHidden().toggleStyle(.switch).tint(Theme.accent)
        }
    }

    private func numberField(_ label: String, _ placeholder: String, _ binding: Binding<String>, icon: String) -> some View {
        FormField(label: label, systemImage: icon) {
            TextField(placeholder, text: binding)
        }
    }

    private func errorBanner(_ message: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "xmark.octagon.fill").foregroundStyle(Theme.loss)
            Text(message).font(.rounded(12, .medium)).foregroundStyle(Theme.loss)
            Spacer()
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 10).fill(Theme.loss.opacity(0.1)))
    }

    // MARK: Load / save

    private func load() async {
        loading = true
        loadError = nil
        defer { loading = false }
        do {
            let config = try await store.loadConfig()
            original = config
            hydrate(from: config)
            loaded = true
        } catch {
            loadError = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        }
    }

    private func hydrate(from c: [String: JSONValue]) {
        defaultIgn = string(c["defaultIgn"])
        startDefaultOnly = bool(c["startDefaultOnly"])
        useCookie = bool(c["useCookie"])
        autoCookie = scalarString(c["autoCookie"])
        angryCoopPrevention = bool(c["angryCoopPrevention"])
        relist = bool(c["relist"])
        pingOnUpdate = bool(c["pingOnUpdate"])
        delay = numberString(c["delay"])
        buyingReadyDelay = numberString(c["buyingReadyDelay"])
        waittime = numberString(c["waittime"])
        percentOfTarget = numberString(c["percentOfTarget"])
        listHours = numberString(c["listHours"])
        clickDelay = numberString(c["clickDelay"])
        bedSpam = bool(c["bedSpam"])
        blockUselessMessages = bool(c["blockUselessMessages"])
        roundTo = numberString(c["roundTo"])
        visitFriend = bool(c["visitFriend"])
        webhookFormat = string(c["webhookFormat"])
        if case .object(let skip)? = c["skip"] {
            skipMinProfit = scalarString(skip["minProfit"])
            skipMinPrice = scalarString(skip["minPrice"])
            skipProfitPercentage = scalarString(skip["profitPercentage"])
        }
    }

    /// Build the patch from only the fields whose edited value differs from the
    /// originally loaded value, so we never resend untouched (or omitted) keys.
    private func save() async {
        guard loaded else { return }
        saving = true
        defer { saving = false }
        var patch: [String: JSONValue] = [:]
        diffBool("startDefaultOnly", startDefaultOnly, into: &patch)
        diffBool("useCookie", useCookie, into: &patch)
        diffBool("angryCoopPrevention", angryCoopPrevention, into: &patch)
        diffBool("relist", relist, into: &patch)
        diffBool("pingOnUpdate", pingOnUpdate, into: &patch)
        diffBool("bedSpam", bedSpam, into: &patch)
        diffBool("blockUselessMessages", blockUselessMessages, into: &patch)
        diffBool("visitFriend", visitFriend, into: &patch)
        diffString("defaultIgn", defaultIgn, into: &patch)
        diffString("webhookFormat", webhookFormat, into: &patch)
        diffScalarString("autoCookie", autoCookie, into: &patch)
        diffNumber("delay", delay, into: &patch)
        diffNumber("buyingReadyDelay", buyingReadyDelay, into: &patch)
        diffNumber("waittime", waittime, into: &patch)
        diffNumber("percentOfTarget", percentOfTarget, into: &patch)
        diffNumber("listHours", listHours, into: &patch)
        diffNumber("clickDelay", clickDelay, into: &patch)
        diffNumber("roundTo", roundTo, into: &patch)
        diffSkip(into: &patch)

        let ok = await store.saveConfig(patch)
        if ok && !patch.isEmpty {
            // Adopt the saved values as the new baseline so further edits diff cleanly.
            for (key, value) in patch { original[key] = value }
        }
    }

    // MARK: Diffing

    private func diffBool(_ key: String, _ value: Bool, into patch: inout [String: JSONValue]) {
        if bool(original[key]) != value { patch[key] = .bool(value) }
    }
    private func diffString(_ key: String, _ value: String, into patch: inout [String: JSONValue]) {
        if string(original[key]) != value { patch[key] = .string(value) }
    }
    /// Like `diffString` but allows numbers/booleans in the original to be
    /// edited as text (e.g. autoCookie can be `false` or `"2h"`).
    private func diffScalarString(_ key: String, _ value: String, into patch: inout [String: JSONValue]) {
        if scalarString(original[key]) != value { patch[key] = .string(value) }
    }
    private func diffNumber(_ key: String, _ value: String, into patch: inout [String: JSONValue]) {
        let trimmed = value.trimmingCharacters(in: .whitespaces)
        guard numberString(original[key]) != trimmed else { return }
        if let n = Double(trimmed) { patch[key] = .number(n) }
    }
    /// Skip thresholds live in a nested object. PATCH merges top-level keys
    /// wholesale, so when any of the three change, resend the whole `skip`
    /// object with the other fields (always, skins, userFinder) preserved.
    private func diffSkip(into patch: inout [String: JSONValue]) {
        var orig: [String: JSONValue] = [:]
        if case .object(let s)? = original["skip"] { orig = s }
        let changed = scalarString(orig["minProfit"]) != skipMinProfit
            || scalarString(orig["minPrice"]) != skipMinPrice
            || scalarString(orig["profitPercentage"]) != skipProfitPercentage
        guard changed else { return }
        var next = orig
        next["minProfit"] = .string(skipMinProfit)
        next["minPrice"] = .string(skipMinPrice)
        next["profitPercentage"] = .string(skipProfitPercentage)
        patch["skip"] = .object(next)
    }

    // MARK: Scalar extraction

    private func bool(_ value: JSONValue?) -> Bool {
        if case .bool(let b)? = value { return b }
        return false
    }
    private func string(_ value: JSONValue?) -> String {
        if case .string(let s)? = value { return s }
        return ""
    }
    private func numberString(_ value: JSONValue?) -> String {
        if case .number(let n)? = value {
            return n == n.rounded() ? String(Int(n)) : String(n)
        }
        if case .string(let s)? = value { return s }
        return ""
    }
    private func scalarString(_ value: JSONValue?) -> String {
        switch value {
        case .string(let s): return s
        case .number(let n): return n == n.rounded() ? String(Int(n)) : String(n)
        case .bool(let b): return b ? "true" : "false"
        default: return ""
        }
    }
}
