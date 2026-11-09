#if DEBUG
import Foundation

/// Sample masked addresses for screenshots and previews (`FeatureFixtures.isOn`).
enum MaskedFixtures {
    static let connection = MaskedConnection(
        connected: true, server: "https://mail.example.com", username: "nyu@example.com",
        domains: ["mask.example.com", "relay.example.net"], defaultDomain: "mask.example.com",
        status: "ok")

    static let notConnected = MaskedConnection(
        connected: false, server: nil, username: nil, domains: nil, defaultDomain: nil, status: nil)

    static let revoked = MaskedConnection(
        connected: true, server: "https://mail.example.com", username: "nyu@example.com",
        domains: nil, defaultDomain: nil, status: "revoked")

    static let addresses: [MaskedAddress] = [
        MaskedAddress(
            id: "m1", email: "shy.fox.17@mask.example.com", state: "enabled",
            forDomain: "shop.example.com", description: "Bestellungen",
            createdAt: "2026-10-28T09:12:00Z", lastMessageAt: "2026-10-29T18:40:00Z",
            cipherId: "i1", itemName: "Example Shop"),
        MaskedAddress(
            id: "m2", email: "quiet.otter.42@mask.example.com", state: "enabled",
            forDomain: "news.example.org", description: nil,
            createdAt: "2026-10-20T11:00:00Z", lastMessageAt: "2026-10-30T07:05:00Z",
            cipherId: nil, itemName: nil),
        MaskedAddress(
            id: "m3", email: "sleepy.panda.08@relay.example.net", state: "disabled",
            forDomain: "forum.example.net", description: "Forum, zu viel Werbung",
            createdAt: "2026-09-02T15:30:00Z", lastMessageAt: "2026-09-30T12:00:00Z",
            cipherId: nil, itemName: nil),
        MaskedAddress(
            id: "m4", email: "brave.koala.93@mask.example.com", state: "pending",
            forDomain: nil, description: "Gewinnspiel",
            createdAt: "2026-10-30T08:00:00Z", lastMessageAt: nil,
            cipherId: nil, itemName: nil),
    ]

    static let items: [ItemSummary] = [
        item("i1", "Example Shop", "nyu@example.com", "shop.example.com"),
        item("i2", "Example News", "nyu", "news.example.org"),
        item("i3", "Example Forum", "nyu.cat", "forum.example.net"),
    ]

    private static func item(_ id: String, _ name: String, _ user: String, _ host: String)
        -> ItemSummary
    {
        ItemSummary(
            id: id, kind: .login, name: name, subtitle: user, host: host, favorite: false,
            folderId: nil, organizationId: nil, collectionIds: [], deleted: false, reprompt: false,
            viewPassword: true, hasTotp: false, hasPassword: true, hasUsername: true, broken: false,
            revisionDate: "2026-10-01T10:00:00Z")
    }
}
#endif
