//! Known answers from Bitwarden's own SDK (bitwarden/sdk-internal,
//! crates/bitwarden-crypto, bitwarden-vault and bitwarden-send): if these
//! hold, a master password typed here produces the same hash and opens the
//! same keys as in Bitwarden's apps, and the same attachments, Sends and
//! fingerprint phrases.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64_URL};
use base64::Engine as _;
use sha2::Digest as _;
use uwulock_core::crypto::{
    decrypt_file, decrypt_user_key, derive_shareable_key, fingerprint, master_key,
    master_password_hash, prf_key, send_key, send_password_hash, EncString, Kdf, PrfKeySet,
    PrivateKey, PublicKey, SymmetricKey,
};
use uwulock_core::passkey::{
    attestation_object, credential_id_from_bytes, rp_id_hash, Passkey, AAGUID, BE, BS, UP, UV,
};
use uwulock_core::Error;
use zeroize::Zeroizing;

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

#[test]
fn fingerprint_matches_bitwarden() {
    let public_key = [
        48, 130, 1, 34, 48, 13, 6, 9, 42, 134, 72, 134, 247, 13, 1, 1, 1, 5, 0, 3, 130, 1, 15, 0,
        48, 130, 1, 10, 2, 130, 1, 1, 0, 187, 38, 44, 241, 110, 205, 89, 253, 25, 191, 126, 84,
        121, 202, 61, 223, 189, 244, 118, 212, 74, 139, 130, 97, 115, 164, 167, 106, 191, 188, 233,
        218, 196, 250, 187, 146, 125, 160, 150, 49, 198, 224, 176, 10, 0, 143, 99, 230, 232, 160,
        51, 104, 154, 211, 33, 80, 170, 4, 68, 80, 219, 115, 167, 114, 156, 227, 125, 193, 128,
        123, 39, 254, 191, 124, 63, 129, 44, 63, 18, 56, 161, 48, 158, 0, 27, 146, 2, 99, 136, 75,
        21, 135, 6, 118, 12, 26, 251, 184, 172, 249, 53, 78, 210, 46, 143, 17, 104, 202, 65, 173,
        229, 219, 233, 144, 163, 101, 216, 238, 152, 54, 158, 1, 195, 50, 203, 21, 226, 12, 82,
        170, 175, 170, 160, 21, 247, 248, 80, 97, 123, 0, 152, 116, 229, 126, 221, 199, 155, 194,
        192, 51, 207, 177, 240, 160, 84, 241, 41, 88, 176, 53, 111, 28, 173, 177, 232, 158, 22, 79,
        133, 152, 31, 32, 12, 196, 147, 58, 57, 50, 252, 208, 131, 150, 179, 132, 178, 150, 234,
        251, 143, 125, 163, 144, 20, 46, 71, 168, 252, 164, 86, 120, 124, 56, 252, 206, 210, 236,
        212, 139, 127, 189, 236, 40, 46, 2, 238, 13, 216, 40, 48, 85, 133, 229, 181, 155, 176, 217,
        241, 154, 153, 213, 112, 222, 72, 219, 197, 3, 219, 56, 77, 109, 47, 72, 251, 131, 36, 240,
        96, 169, 31, 82, 93, 166, 242, 3, 33, 213, 2, 3, 1, 0, 1,
    ];
    assert_eq!(
        fingerprint("a09726a0-9590-49d1-a5f5-afe300b6a515", &public_key),
        "turban-deftly-anime-chatroom-unselfish"
    );
}

// Bitwarden's SDK has no known answer for a passkey's key set, so this one was
// made independently, with Python's `cryptography`, the way Bitwarden's web
// vault makes it (WebAuthnLoginPrfKeyService, DefaultRotateableKeySetService):
// a fresh RSA-2048 key pair, the user key RSA-OAEP-SHA1-wrapped for it, the
// public key under the user key, the private key under the stretched PRF output.
const PRF: &str = "zneACW8mIItRUUPXmFYNTALq1IgEqNDzUPpkVkc2J5c=";
const PRF_USER_KEY: &str =
    "n881qAvoh5N+nxB1WcHmkTxXnJCsKIC/tT8D1qlqnmuPrAQrm3QcDYk9dK87tZAk7Ae4/JvYk2VXwHFpiWRBlg==";
const PRF_ENCRYPTED_USER_KEY: &str = "4.LdwYwZNp3+jcmwPP2U4fs4WUvsRr5J/PaUXJWYzOUD5jkdHDHhv0ZlH08a5p64/WB2Lq4TPzlHvh0x9VxBu7E8yAxqFkcllcY3tOufkgfcDeTNtLFvRvaT2c4feiunoIa8qlQQgMXKlIcIYPA+nETm5lJ/ms5N4z+iAtBpFmFLCPCefCD6SsplOr+rcQ+Jw2kHL8/gNWVc38OWrqQSIjzHrb1XbinDU6Pbwved0MdA3ycXZVnTPYSGqJkrW1DCI5KY3EqNnMcZKRa6mwwPkESO8psagyNDTugkbYkMLK/5cdJ5hIaIzIAvqVB4Lxxa4pvxREjWII3slH4X3v2eWWTQ==";
const PRF_ENCRYPTED_PUBLIC_KEY: &str = "2.AAECAwQFBgcICQoLDA0ODw==|SeQlackjlQ29Vy1Vt1FWkFWmJcapjFL8VdjRFQx2NlID52FHCT5dLrp+4UIiVgh6qV9Yo+Q1mVAt9mmMK7TdnsKFTUwEyEL3FVxwZ5NSpw2LKCEYr4PZE+kHRApD/HbQyzfqQruUcMeGUauSny5wLiCIc1MshgnfQqMvynLUW6ParQa5qyxUg3t0kvBWGMS4+hERHHU+HhZRvo+Zwm8s4E5Dq/unv1aw8qR5ulOl4++O3OomFjAZeXmLqaUm+A4z4vcS6LDVQ2bYU0lUyBDTp6rL0knCTrkOzKfTWtfQrA4YtIS8B3NKtobosFLogQ96Z2Bjgwg7zcm+0dFTkXNJNOYMeVF9qW5odJo6Fs8CYwvF09+gyhU0b2Q42tmg9q8hsI+3BV4bad0NsLBTztbUpA==|j09lw9J2qNN3KY9DNp+OJ4cqLnjIWxMeqpDjZT9Dn6s=";
const PRF_ENCRYPTED_PRIVATE_KEY: &str = "2.EBESExQVFhcYGRobHB0eHw==|MRv+6J8ey3rAbNQARlK5Tvmiuucw41dxcFxNvgPQFqAnTa56wXWA6a7vMw4aoTpbuPCOVAHKkC2fxoyYvoTFxxG229s4R6L39fRvW9f+j1B1bGA0ORom/SFQU74dhwHCe9pW1GB+mKaIOpDQnHXTgNh7mzSA71bJYOPMYn0wEokHloAWO9G+8u8ZZhe/kYrkYhNbAdkZSsx8q67wdMOAQXGTsrhCTnO7tyEzsNXXhG9olXj3MWe8qk0nP/4y05R7nx66qz4SxM7NCRu/YsXKo2YRD5qW9Qb9yCHXxCHbC0Wnrji7mEiSoSdXZdRvyUHeimTpd9s7x/E/qZW233WFifWnmGtQNrje0NEB6S4r0Iv1WVdez1l+VAUZeWDvg8O7TElm76dOzaIqdRaPDGWv/VMvkibzSdmYz+fj77W/8ynX5VCkGqs+qxZzbLpLo+LHDRzY46n7Vd0YR162BGf0hT61kJEuSgE0BouvTre/mx7YJyXPo4UqVhzlv1ZdpiwjI5P+X8RNk2dlh7s93y0Jpf2WEqrM8VZKW9nm5mgLYFJPT8rnvEyhNo52IqahuDWShCNcEcK9074eD78UT9bjDEmDjaWYi7GJT/Kcec6gEVtpvE+YGFgSVKvYapKKPKUQSI7QNmeh3o8essKkOLwskmJsdX9TPGK+ktRfOeJFwgshk0P5pQ1Pk/RDjCNeypT+n0OOq2BGtuuuHakoVtkRaKOVEXLvgjgtlOwi+0oLH2wlKM9JBV8+Qo2BjBOi//mk0gAyyF6jY0D3uKMRv+1k5tr4rGJOiAEexuNADcm2M8Ymm4OcAEXQIKG+EG3l/12ge3sHyne3kHEVjojmqZ0bO3X5VYGeGa4MA1rSBXCLNFlyMDoZPl57YkVDy3OtjAxqsk3mvqGHWO9y76Q81fO7/gfkCE/y6BtUWMmfExhGZtjzzdbXoTX+a+MJRzmymt1h/bvnqH5CH3lVRe6fuHo8LRY/FxomA8CO2UFIMylTYEAHhQsFMSrSBrn/WNWjJDevmVDr6bVnJrTMlNZKfjRPbXeFUTeCpl5ATwpUzYDuBldT3z1cBLMDblTSpLImeUePi5upQ57+PSwBrAFX/RhCNl3EYnpb/c+EwdbBhBNZDslc3iPHAlZ3iKG0zH1D01ZCw+TDeVbAo7ReISSM3O1lc+Utyh4sUM1Gr8AHOEfcNkzt/DacahOVQGmvsDTWiZtWTJVkImsHtK5cGgVyU0AsBRtxXKpS8V4psPstbOcnOzsAwM88ivY9wtcrbmUgtDxupa+i/hFOFzSS6kERAdm8Ta5p75DlwCaQ5g8JAJTD/qaxqO7oajBs6DkImR1wDTG4iA4QqrmONIqM8qDyFz4ClhsbZ+6uSqOy2bX72xNFFoKqwGfq9OQnmQ9bUel/11o4dctuOtiLwBaK8/cMSbLL4mJjLyQ5PclxKjlpAKGLlJN75zmsyOVgFYd/sJUPWXooF3ioQmVCjMYobjhbV5HV+4LMdZ70vhGL0XXYvtknxg41moYCmp7MTehQuqST/3hBGK1y799dQiQdosYlEGf+ehDAhCTaub9w9/QQOngkGcFpuyxtIVXe+vH1vlqJ/w/EkUa3UxmpbDtEGZJBJOG699X+hTFct47Tag5D6zkyT+w=|dQJzpz+QG8tPd63YH8Wp3DpmqYBc6fssQJIQQLcWBxA=";

#[test]
fn prf_key_is_the_stretched_prf_output() {
    let prf = B64.decode(PRF).unwrap();
    assert_eq!(
        B64.encode(prf_key(&prf).unwrap().to_bytes()),
        "OBf2A92aPP/LzVHuF7twpF01JrriYfmJfrrPJHLneUsPLmQFs77RNsrK2r0XF2qCNGzmbGSWrGKghhRWa2QC6A=="
    );
}

#[test]
fn opens_a_passkey_key_set_made_elsewhere() {
    let prf = B64.decode(PRF).unwrap();
    let set = PrfKeySet {
        encrypted_user_key: PRF_ENCRYPTED_USER_KEY.parse().unwrap(),
        encrypted_public_key: PRF_ENCRYPTED_PUBLIC_KEY.parse().unwrap(),
        encrypted_private_key: PRF_ENCRYPTED_PRIVATE_KEY.parse().unwrap(),
    };
    let user = set.open(&prf).unwrap();
    assert_eq!(B64.encode(user.to_bytes()), PRF_USER_KEY);

    // The public half, under the user key, is the private half's.
    let public = set.encrypted_public_key.decrypt(&user).unwrap();
    let private = set
        .encrypted_private_key
        .decrypt(&prf_key(&prf).unwrap())
        .unwrap();
    assert_eq!(
        PrivateKey::from_der(&private).unwrap().public(),
        PublicKey::from_der(&public).unwrap()
    );
    // Computed there as well.
    assert_eq!(
        fingerprint("nyu@example.com", &public),
        "unstable-freemason-banshee-sedation-zips"
    );

    // Another passkey's output doesn't open it.
    let mut other = prf.clone();
    other[0] ^= 1;
    assert!(matches!(set.open(&other), Err(Error::WrongKey)));
}

// A passkey that signs in, made independently with Python's `cryptography`:
// the P-256 key with the private scalar 01 02 … 20, exported as PKCS#8 the way
// WebCrypto exports it for Bitwarden, and its public key. The signature is
// RFC 6979's deterministic ECDSA over "authenticator data" and the SHA-256 of
// "client data", so any correct implementation gives the same bytes.
const PASSKEY_PKCS8: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgAQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyChRANCAARRXD1uueOWuQTT_sp_VP3NDMHpl783XcpRWtCmw7QDX0U2vjpQ8xj7-aVHWQKiIVAr7w1X4IxTsswKVvF9n5NU";
const PASSKEY_SPKI: &str = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEUVw9brnjlrkE0/7Kf1T9zQzB6Ze/N13KUVrQpsO0A19FNr46UPMY+/mlR1kCoiFQK+8NV+CMU7LMClbxfZ+TVA==";
const PASSKEY_X: &str = "515c3d6eb9e396b904d3feca7f54fdcd0cc1e997bf375dca515ad0a6c3b4035f";
const PASSKEY_Y: &str = "4536be3a50f318fbf9a5475902a221502bef0d57e08c53b2cc0a56f17d9f9354";
const PASSKEY_SIGNATURE: &str = "304502203386bd7c1855443e9f9731e67fdae8af3e8ac321c87b4632d3a486260e9c2059022100da68726941f33e3ab9007628daf4f8f845d8e30a65122a246aef967163afe0e4";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fixed_passkey(key_value: &str) -> Passkey {
    Passkey {
        credential_id: "b2a5d2c1-5e3f-4a7b-9c1d-0e2f4a6b8c9d".into(),
        key_value: Zeroizing::new(key_value.into()),
        rp_id: "example.com".into(),
        rp_name: None,
        user_handle: None,
        user_name: None,
        user_display_name: None,
        counter: 0,
        discoverable: true,
        creation_date: String::new(),
    }
}

#[test]
fn a_passkeys_public_key_and_signature_match() {
    // Bitwarden's URL-safe base64, and standard base64 with padding as some
    // imports write it.
    let padded = B64.encode(B64_URL.decode(PASSKEY_PKCS8).unwrap());
    for key_value in [PASSKEY_PKCS8, padded.as_str()] {
        let passkey = fixed_passkey(key_value);
        assert_eq!(B64.encode(passkey.public_key_spki().unwrap()), PASSKEY_SPKI);
        let cose = passkey.public_key_cose().unwrap();
        assert_eq!(
            hex(&cose),
            format!("a5010203262001215820{PASSKEY_X}225820{PASSKEY_Y}")
        );
        let message = sha2::Sha256::digest(b"client data");
        assert_eq!(
            hex(&passkey.sign(b"authenticator data", &message).unwrap()),
            PASSKEY_SIGNATURE
        );
    }
}

#[test]
fn rp_id_hash_is_sha256() {
    // `printf example.com | sha256sum`.
    assert_eq!(
        hex(&rp_id_hash("example.com")),
        "a379a6f6eeafb9a55e378c118034e2751e682fab9f2d30ab13d2125586ce1947"
    );
}

#[test]
fn a_passkeys_attested_data_as_a_site_reads_it() {
    let passkey = fixed_passkey(PASSKEY_PKCS8);
    let data = passkey.authenticator_data(UP | UV | BE | BS, true).unwrap();
    let object = attestation_object(&data);
    let expected = format!(
        "a363666d74646e6f6e656761747453746d74a06861757468446174615894{}5d{}{}0010{}{}",
        "a379a6f6eeafb9a55e378c118034e2751e682fab9f2d30ab13d2125586ce1947",
        "00000000",
        hex(&AAGUID),
        "b2a5d2c15e3f4a7b9c1d0e2f4a6b8c9d",
        format_args!("a5010203262001215820{PASSKEY_X}225820{PASSKEY_Y}"),
    );
    assert_eq!(hex(&object), expected);
    // Bitwarden's GUID ↔ bytes: the bytes in the order they are written.
    assert_eq!(
        credential_id_from_bytes(&passkey.credential_id_bytes().unwrap()),
        passkey.credential_id
    );
}

fn key(base64: &str) -> SymmetricKey {
    SymmetricKey::from_bytes(&B64.decode(base64).unwrap()).unwrap()
}
