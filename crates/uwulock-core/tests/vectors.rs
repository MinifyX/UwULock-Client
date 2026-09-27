//! Known answers from Bitwarden's own SDK (bitwarden/sdk-internal,
//! crates/bitwarden-crypto, bitwarden-vault and bitwarden-send): if these
//! hold, a master password typed here produces the same hash and opens the
//! same keys as in Bitwarden's apps, and the same attachments and Sends.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64_URL};
use base64::Engine as _;
use uwulock_core::crypto::{
    decrypt_file, decrypt_user_key, derive_shareable_key, master_key, master_password_hash,
    send_key, send_password_hash, EncString, Kdf, SymmetricKey,
};

#[test]
fn pbkdf2_hash_matches_bitwarden() {
    // Email case and surrounding space make no difference.
    for email in [
        "test@bitwarden.com",
        "TEST@bitwarden.com",
        " test@bitwarden.com",
    ] {
        let key = master_key(
            "asdfasdf",
            email,
            Kdf::Pbkdf2 {
                iterations: 100_000,
            },
        )
        .unwrap();
        assert_eq!(
            master_password_hash(&key, "asdfasdf"),
            "wmyadRMyBZOH7P/a/ucTCbSghKgdzDpPqUnu/DAVtSw="
        );
    }
}

#[test]
fn argon2id_hash_matches_bitwarden() {
    let kdf = Kdf::Argon2id {
        iterations: 4,
        memory_mib: 32,
        parallelism: 2,
    };
    let key = master_key("asdfasdf", "test_salt", kdf).unwrap();
    assert_eq!(
        master_password_hash(&key, "asdfasdf"),
        "PR6UjYmjmppTYcdyTiNbAhPJuQQOmynKbdEl1oyi/iQ="
    );
}

#[test]
fn stretching_matches_bitwarden() {
    let master = [
        31, 79, 104, 226, 150, 71, 177, 90, 194, 80, 172, 209, 17, 129, 132, 81, 138, 167, 69, 167,
        254, 149, 2, 27, 39, 197, 64, 42, 22, 195, 86, 75,
    ];
    let stretched = SymmetricKey::stretch(&master).to_bytes();
    assert_eq!(
        &stretched[..32],
        [
            111, 31, 178, 45, 238, 152, 37, 114, 143, 215, 124, 83, 135, 173, 195, 23, 142, 134,
            120, 249, 61, 132, 163, 182, 113, 197, 189, 204, 188, 21, 237, 96
        ]
    );
    assert_eq!(
        &stretched[32..],
        [
            221, 127, 206, 234, 101, 27, 202, 38, 86, 52, 34, 28, 78, 28, 185, 16, 48, 61, 127,
            166, 209, 247, 194, 87, 232, 26, 48, 85, 193, 249, 179, 155
        ]
    );
}

#[test]
fn opens_a_legacy_user_key() {
    let key = master_key(
        "asdfasdfasdf",
        "legacy@bitwarden.com",
        Kdf::Pbkdf2 {
            iterations: 600_000,
        },
    )
    .unwrap();
    let protected: EncString = "0.8UClLa8IPE1iZT7chy5wzQ==|6PVfHnVk5S3XqEtQemnM5yb4JodxmPkkWzmDRdfyHtjORmvxqlLX40tBJZ+CKxQWmS8tpEB5w39rbgHg/gqs0haGdZG4cPbywsgGzxZ7uNI=".parse().unwrap();
    let user = decrypt_user_key(&key, &protected).unwrap().to_bytes();
    assert_eq!(
        &user[..32],
        [
            12, 95, 151, 203, 37, 4, 236, 67, 137, 97, 90, 58, 6, 127, 242, 28, 209, 168, 125, 29,
            118, 24, 213, 44, 117, 202, 2, 115, 132, 165, 125, 148
        ]
    );
    assert_eq!(
        &user[32..],
        [
            186, 215, 234, 137, 24, 169, 227, 29, 218, 57, 180, 237, 73, 91, 189, 51, 253, 26, 17,
            52, 226, 4, 134, 75, 194, 208, 178, 133, 128, 224, 140, 167
        ]
    );

    // And the wrong password doesn't.
    let wrong = master_key(
        "asdfasdfasdX",
        "legacy@bitwarden.com",
        Kdf::Pbkdf2 {
            iterations: 600_000,
        },
    )
    .unwrap();
    assert!(decrypt_user_key(&wrong, &protected).is_err());
}

#[test]
fn opens_a_current_user_key() {
    let key = master_key(
        "hunter22hunter22",
        "nyu@example.com",
        Kdf::Pbkdf2 {
            iterations: 600_000,
        },
    )
    .unwrap();
    let user = SymmetricKey::generate();
    let protected = EncString::encrypt(&user.to_bytes(), &SymmetricKey::stretch(&key));
    let opened = decrypt_user_key(&key, &protected.to_string().parse().unwrap()).unwrap();
    assert_eq!(opened.to_bytes().as_slice(), user.to_bytes().as_slice());
}

#[test]
fn shareable_keys_match_bitwarden() {
    let key = derive_shareable_key(b"&/$%F1a895g67HlX", "test_key", None);
    assert_eq!(
        B64.encode(key.to_bytes()),
        "4PV6+PcmF2w7YHRatvyMcVQtI7zvCyssv/wFWmzjiH6Iv9altjmDkuBD1aagLVaLezbthbSe+ktR+U6qswxNnQ=="
    );
    let key = derive_shareable_key(b"67t9b5g67$%Dh89n", "test_key", Some("test"));
    assert_eq!(
        B64.encode(key.to_bytes()),
        "F9jVQmrACGx9VUPjuzfMYDjr726JtL300Y3Yg+VYUnVQtQ1s8oImJ5xtp1KALC9h2nav04++1LDW4iFD+infng=="
    );
}

#[test]
fn send_key_matches_bitwarden() {
    let user = key(
        "w2LO+nwV4oxwswVYCxlOfRUseXfvU03VzvKQHrqeklPgiMZrspUe6sOBToCnDn9Ay0tuCBn8ykVVRb7PWhub2Q==",
    );
    let seed: EncString = "2.+1KUfOX8A83Xkwk1bumo/w==|Nczvv+DTkeP466cP/wMDnGK6W9zEIg5iHLhcuQG6s+M=|SZGsfuIAIaGZ7/kzygaVUau3LeOvJUlolENBOU+LX7g=".parse().unwrap();
    let send = send_key(&seed.decrypt(&user).unwrap()).unwrap();
    assert_eq!(
        B64.encode(send.to_bytes()),
        "IR9ImHGm6rRuIjiN7csj94bcZR5WYTJj5GtNfx33zm6tJCHUl+QZlpNPba8g2yn70KnOHsAODLcR0um6E3MAlg=="
    );
    assert!(send_key(&[0; 32]).is_err());
}

#[test]
fn opens_a_send_as_bitwarden_wrote_it() {
    let user = key(
        "bYCsk857hl8QJJtxyRK65tjUrbxKC4aDifJpsml+NIv4W9cVgFvi3qVD+yJTUU2T4UwNKWYtt9pqWf7Q+2WCCg==",
    );
    let seed = "2.KLv/j0V4Ebs0dwyPdtt4vw==|jcrFuNYN1Qb3onBlwvtxUV/KpdnR1LPRL4EsCoXNAt4=|gHSywGy4Rj/RsCIZFwze4s2AACYKBtqDXTrQXjkgtIE="
        .parse::<EncString>()
        .unwrap()
        .decrypt(&user)
        .unwrap();
    // The seed is what the Send's link carries after the `#`.
    assert_eq!(B64_URL.encode(&seed), "Pgui0FK85cNhBGWHAlBHBw");
    let send = send_key(&seed).unwrap();
    let name: EncString = "2.STIyTrfDZN/JXNDN9zNEMw==|NDLum8BHZpPNYhJo9ggSkg==|UCsCLlBO3QzdPwvMAWs2VVwuE6xwOx/vxOooPObqnEw=".parse().unwrap();
    let text: EncString = "2.2VPyLzk1tMLug0X3x7RkaQ==|mrMt9vbZsCJhJIj4eebKyg==|aZ7JeyndytEMR1+uEBupEvaZuUE69D/ejhfdJL8oKq0=".parse().unwrap();
    assert_eq!(name.decrypt_string(&send).unwrap().as_str(), "Test");
    assert_eq!(
        text.decrypt_string(&send).unwrap().as_str(),
        "This is a test"
    );
}

#[test]
fn send_password_hash_matches_bitwarden() {
    let seed = B64_URL.decode("Pgui0FK85cNhBGWHAlBHBw").unwrap();
    assert_eq!(
        send_password_hash("abc123", &seed),
        "vTIDfdj3FTDbejmMf+mJWpYdMXsxfeSd1Sma3sjCtiQ="
    );
}

#[test]
fn opens_attachments_as_bitwarden_wrote_them() {
    let user = key(
        "w2LO+nwV4oxwswVYCxlOfRUseXfvU03VzvKQHrqeklPgiMZrspUe6sOBToCnDn9Ay0tuCBn8ykVVRb7PWhub2Q==",
    );
    let original = B64.decode("rMweTemxOL9D0iWWfRxiY3enxiZ5IrwWD6ef2apGO6MvgdGhy2fpwmATmn7BpSj9lRumddLLXm7u8zSp6hnXt1hS71YDNh78LjGKGhGL4sbg8uNnpa/I6GK/83jzqGYN7+ESbg==").unwrap();

    // With an attachment key, under the item key, under the user key.
    let item = "2.Gg8yCM4IIgykCZyq0O4+cA==|GJLBtfvSJTDJh/F7X4cJPkzI6ccnzJm5DYl3yxOW2iUn7DgkkmzoOe61sUhC5dgVdV0kFqsZPcQ0yehlN1DDsFIFtrb4x7LwzJNIkMgxNyg=|1rGkGJ8zcM5o5D0aIIwAyLsjMLrPsP3EWm3CctBO3Fw="
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&user)
        .unwrap();
    let attachment = "2.r288/AOSPiaLFkW07EBGBw==|SAmnnCbOLFjX5lnURvoualOetQwuyPc54PAmHDTRrhT0gwO9ailna9U09q9bmBfI5XrjNNEsuXssgzNygRkezoVQvZQggZddOwHB6KQW5EQ=|erIMUJp8j+aTcmhdE50zEX+ipv/eR1sZ7EwULJm/6DY="
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&item)
        .unwrap();
    let file = B64.decode("Ao00qr1xLsV+ZNQpYZ/UwEwOWo3hheKwCYcOGIbsorZ6JIG2vLWfWEXCVqP0hDuzRvmx8otApNZr8pJYLNwCe1aQ+ySHQYGkdubFjoMojulMbQ959Y4SJ6Its/EnVvpbDnxpXTDpbutDxyhxfq1P3lstL2G9rObJRrxiwdGlRGu1h94UA1fCCkIUQux5LcqUee6W4MyQmRnsUziH8gGzmtI=").unwrap();
    assert_eq!(
        decrypt_file(&file, &attachment).unwrap().as_slice(),
        original
    );
    assert!(decrypt_file(&file, &item).is_err());

    // An old one, under the user key itself.
    let file = B64.decode("AsQLXOBHrJ8porroTUlPxeJOm9XID7LL9D2+KwYATXEpR1EFjLBpcCvMmnqcnYLXIEefe9TCeY4Us50ux43kRSpvdB7YkjxDKV0O1/y6tB7qC4vvv9J9+O/uDEnMx/9yXuEhAW/LA/TsU/WAgxkOM0uTvm8JdD9LUR1z9Ql7zOWycMVzkvGsk2KBNcqAdrotS5FlDftZOXyU8pWecNeyA/w=").unwrap();
    assert_eq!(decrypt_file(&file, &user).unwrap().as_slice(), original);
}

fn key(base64: &str) -> SymmetricKey {
    SymmetricKey::from_bytes(&B64.decode(base64).unwrap()).unwrap()
}
