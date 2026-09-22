//! Known answers from Bitwarden's own SDK (bitwarden/sdk-internal,
//! crates/bitwarden-crypto): if these hold, a master password typed here
//! produces the same hash and opens the same keys as in Bitwarden's apps.

use uwulock_bitwarden::crypto::{
    decrypt_user_key, master_key, master_password_hash, EncString, Kdf, SymmetricKey,
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
