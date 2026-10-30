// The store the app and the AutoFill extension share (CredentialProvider/PasskeyVault.swift),
// against a temporary folder: no Keychain, no host app. The byte layouts are the app's
// (crates/…/passkeys/apple.rs), so a change here is a change to the protocol.

import AuthenticationServices
import CryptoKit
import Foundation
import Testing

@Suite struct PasskeyVaultTests {
    let key = SymmetricKey(size: .bits256)
    /// Its own account per test: the generation seen is kept in UserDefaults.
    let account = "acct-" + UUID().uuidString.prefix(8).lowercased()
    let folder: URL
    let vault: PasskeyVault

    init() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("uwulock-passkeys-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        vault = PasskeyVault(folder: folder, keychainGroup: "TEAMID.app.uwulock.passkeys")
    }

    private func write(_ snapshot: PasskeySnapshot, account: String? = nil) throws {
        try PasskeyVault.seal(
            JSONEncoder().encode(snapshot), key: key, kind: .list, account: account ?? self.account
        ).write(to: vault.listURL)
    }

    private func login(_ uris: [(String, String)], id: String = "item-1", password: String = "hunter2")
        throws -> PasskeyLoginEntry
    {
        let box = try AES.GCM.seal(
            Data(password.utf8), using: key,
            authenticating: PasskeyVault.passwordAAD(account: account, itemId: id))
        return PasskeyLoginEntry(
            itemId: id, name: "Example", userName: "alex",
            uris: uris.map { PasskeyUriHint(kind: $0.0, value: $0.1) }, subtitle: nil, hosts: nil,
            sealedPassword: Base64URL.encode(box.combined!))
    }

    // MARK: Sealing

    @Test func sealOpensWithItsKindOnly() throws {
        let sealed = try PasskeyVault.seal(Data("hello".utf8), key: key, kind: .list, account: account)
        #expect(sealed[0] == 0x02)
        #expect(Int(sealed[1]) == account.utf8.count)
        #expect(PasskeyVault.accountOf(sealed) == account)
        let opened = try PasskeyVault.open(sealed, key: key, kind: .list)
        #expect(opened.account == account)
        #expect(opened.plain == Data("hello".utf8))
        #expect(throws: (any Error).self) { try PasskeyVault.open(sealed, key: key, kind: .outbox) }
        #expect(throws: (any Error).self) {
            try PasskeyVault.open(sealed, key: SymmetricKey(size: .bits256), kind: .list)
        }
    }

    @Test func aadIsTheAppsDomain() {
        #expect(PasskeyVault.aad(.list, account: "a") == Data("uwulock-passkeys-v2:list:a".utf8))
        #expect(PasskeyVault.aad(.outbox, account: "a") == Data("uwulock-passkeys-v2:outbox:a".utf8))
        #expect(PasskeyVault.keyAAD(account: "a", credentialId: "c") == Data("uwulock-passkeys-v2:key:a:c".utf8))
        #expect(PasskeyVault.passwordAAD(account: "a", itemId: "i") == Data("uwulock-passkeys-v2:password:a:i".utf8))
    }

    @Test func accountsAreChecked() throws {
        #expect(PasskeyVault.validAccount("a-B_9"))
        #expect(!PasskeyVault.validAccount(""))
        #expect(!PasskeyVault.validAccount("a/b"))
        #expect(!PasskeyVault.validAccount(String(repeating: "a", count: 65)))
        #expect(PasskeyVault.accountOf(Data([0x01, 1, 0x61])) == nil)
        #expect(PasskeyVault.accountOf(Data([0x02, 5, 0x61])) == nil)
        #expect(throws: (any Error).self) {
            try PasskeyVault.seal(Data(), key: key, kind: .list, account: "no spaces")
        }
    }

    // MARK: The list

    @Test func noListIsNoList() {
        #expect(throws: PasskeyVaultError.noList) { try vault.list(key: key) }
    }

    @Test func listOpensAndRefusesAnOlderOne() throws {
        try write(PasskeySnapshot(version: 1, generation: 20, entries: [], logins: [try login([("domain", "example.com")])]))
        let list = try vault.list(key: key)
        #expect(list.account == account)
        #expect(list.snapshot.logins?.count == 1)
        try write(PasskeySnapshot(version: 1, generation: 10, entries: [], logins: nil))
        #expect(throws: PasskeyVaultError.stale) { try vault.list(key: key) }
    }

    @Test func passwordOpensForItsItemOnly() throws {
        let entry = try login([], id: "item-1", password: "correct horse")
        #expect(try PasskeyVault.password(entry, key: key, account: account) == "correct horse")
        var moved = entry
        moved.itemId = "item-2"
        #expect(throws: (any Error).self) { try PasskeyVault.password(moved, key: key, account: account) }
    }

    // MARK: Matching

    @Test func matchesLikeAutofillRs() throws {
        func domain(_ host: String) -> ASCredentialServiceIdentifier { .init(identifier: host, type: .domain) }
        func url(_ text: String) -> ASCredentialServiceIdentifier { .init(identifier: text, type: .URL) }

        let byDomain = try login([("domain", "example.com")])
        #expect(PasskeyVault.matches(byDomain, [domain("example.com")]))
        #expect(PasskeyVault.matches(byDomain, [url("https://shop.example.com/login")]))
        #expect(!PasskeyVault.matches(byDomain, [domain("notexample.com")]))
        #expect(!PasskeyVault.matches(byDomain, [domain("example.org")]))

        let byHost = try login([("host", "app.example.test:8443")])
        #expect(PasskeyVault.matches(byHost, [url("https://app.example.test:8443/")]))
        #expect(!PasskeyVault.matches(byHost, [url("https://app.example.test/")]))

        let byPrefix = try login([("startsWith", "https://example.net/admin")])
        #expect(PasskeyVault.matches(byPrefix, [url("https://example.net/admin/users")]))
        #expect(!PasskeyVault.matches(byPrefix, [url("http://example.net/admin")]))
        #expect(!PasskeyVault.matches(byPrefix, [domain("example.net")]))

        let exact = try login([("exact", "https://example.org/a")])
        #expect(PasskeyVault.matches(exact, [url("https://example.org/a")]))
        #expect(!PasskeyVault.matches(exact, [url("https://example.org/a/b")]))
    }

    @Test func rpIdsAsTheAppTakesThem() {
        #expect(PasskeyVault.validRpId("example.com"))
        #expect(PasskeyVault.validRpId("login.example.test"))
        #expect(PasskeyVault.validRpId("localhost"))
        #expect(!PasskeyVault.validRpId("Example.com"))
        #expect(!PasskeyVault.validRpId("example"))
        #expect(!PasskeyVault.validRpId("192.0.2.1"))
        #expect(!PasskeyVault.validRpId("-a.example.com"))
        #expect(!PasskeyVault.validRpId("a..example.com"))
    }

    // MARK: Making a passkey

    @Test func keepWritesTheOutboxAndTheList() throws {
        try write(PasskeySnapshot(version: 1, generation: 5, entries: [], logins: nil))
        let list = try vault.list(key: key)
        let made = PasskeyVault.make(rpId: "example.com", userName: "alex", userHandle: Data([1, 2, 3]))
        #expect(made.id.count == 16)
        let kept = try vault.keep(made.entry, privateKey: made.key, list: list, key: key)
        #expect(kept.snapshot.entries.count == 1)
        #expect(kept.snapshot.generation > 5)

        let outbox = try FileManager.default.contentsOfDirectory(at: vault.outboxURL, includingPropertiesForKeys: nil)
        #expect(outbox.count == 1)
        let opened = try PasskeyVault.open(Data(contentsOf: outbox[0]), key: key, kind: .outbox)
        let entry = try JSONDecoder().decode(PasskeyEntry.self, from: opened.plain)
        #expect(entry.rpId == "example.com")
        #expect(entry.generation == kept.snapshot.generation)

        // The list on disk is the new one, and its key signs.
        let reread = try vault.list(key: key)
        #expect(reread.snapshot.entries.first?.credentialId == Base64URL.encode(made.id))
        let signing = try PasskeyVault.signingKey(reread.snapshot.entries[0], key: key, account: account)
        #expect(signing.publicKey.rawRepresentation == made.key.publicKey.rawRepresentation)
    }

    @Test func authenticatorDataLayout() throws {
        let flags = PasskeyVault.up | PasskeyVault.uv | PasskeyVault.be | PasskeyVault.bs
        let plain = PasskeyVault.authenticatorData(rpId: "example.com", flags: flags)
        #expect(plain.count == 37)
        #expect(plain.prefix(32) == PasskeyVault.sha256(Data("example.com".utf8)))
        #expect(plain[32] == 0x1d)

        let key = P256.Signing.PrivateKey()
        let id = Data(repeating: 7, count: 16)
        let attested = PasskeyVault.authenticatorData(rpId: "example.com", flags: flags, credentialId: id, publicKey: key.publicKey)
        // 37 + aaguid 16 + length 2 + id 16 + COSE key 77
        #expect(attested.count == 37 + 16 + 2 + 16 + 77)
        #expect(attested[32] == 0x5d)
        #expect(attested[37..<53] == PasskeyVault.aaguid)

        let object = PasskeyVault.attestationObject(attested)
        #expect(object.first == 0xa3)
        #expect(object.suffix(attested.count) == attested)

        let hash = PasskeyVault.sha256(Data("client data".utf8))
        let der = try PasskeyVault.sign(key, authData: plain, clientDataHash: hash)
        let signature = try P256.Signing.ECDSASignature(derRepresentation: der)
        #expect(key.publicKey.isValidSignature(signature, for: plain + hash))
    }
}
