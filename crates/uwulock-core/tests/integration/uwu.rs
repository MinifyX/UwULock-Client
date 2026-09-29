//! UwULock's own crypto — the extras key, suite space keys, file requests —
//! against values made independently with Python's `cryptography` (HKDF,
//! AES-CBC with HMAC-SHA256, RSA-OAEP-SHA1), the way the contract
//! (UwULock-Server `docs/uwu-api.md` §3, §6, §11) describes them. The web vault
//! of UwULock-Server makes the uploader's side of a file request; if these
//! hold, what it makes opens here and the other way round.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::json;
use uwulock_core::crypto::{
    decrypt_file, wrap_for, EncString, PrivateKey, PublicKey, SymmetricKey,
};
use uwulock_core::extras::{
    self, open_icon, reencrypt_version, resolve, seal_icon, Keys, Resolved, SpaceKey,
    WrappedExtrasKey,
};
use uwulock_core::file_request::{LinkSecret, PublicInfo, SealedFile, Sender, SubmissionKey};

const SECRET: &str = "AAECAwQFBgcICQoLDA0ODw==";
const LINK_KEY: &str =
    "lgj+EESLBSCiCgCj2DdyKWOifaGeGzcBuxHIdAWvpm1WKzLs1BSM6oul1+TDB+Dn0W0x3EkziqphrGBc39F2/g==";
const PRIVATE: &str = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC2EmCSTyx6YUpNZSRI60oly0VN2cZ9Z4LFw3CuK6zIyVdFW4nGS3R4Ml5X5pdIc7lVn2FNmpi2j/1/TKFZymm/Kb4cgTmRiImF1Gc2OO9v5xlcFyJHDW0Jl8kL3fHNZvz+8ajCtXcVa29GuqCQIdoPgLEYTfCzqhSQc5T77X3QD1+DoO2nY3kXU7t+1GeXMgfUfcEJv0YPjGoofJZMP8GzMJijJTVTJc+M0WhRLPmm6XCr9E0m3OXZxNynYz6euGtPAmfm/ld5QQ+Gu+XYMnZvthAdmzwy9HyMv4cqtFVB8Q2IJOemKymg1ADNLQ453gZMXRDRSUG7kLsA2Ddsusp9AgMBAAECggEAOckbXVRDiZPXQTkYiwwiPFyHYm370VFI7/tXh+/UpuVADYM/9u97x6o0xzEoUpZn/ATZnQez8D1C92Qa0aSsaz+UVvesjcQH4bHIEC2B0MJICjJNbr+UG7dQ17NZSxektEV+ik2Nvf6bEpeo3hXgX4s4qb4S5vLUFASbBFob1CyDtjXZyL+vXiyKd8VxIF4tkG/3E/BUkJ5WANWa0psTxNbFfuOnV87lFo7Cycmx0ynZGIYdfzhz1Aq4f6cQiPdy3cDRuYWw0gKqwwrpFrMpAo2O+x8cVkS2oltZP0GkNoTZLIgcwVAhEE7K2qX1w46Hc0wjf/WlWGjoIvmBnQgDAQKBgQDhwEp56NCveTWfBSav6ZMy1YM44eJXtbUhTMRgSQnkloThnH2MXZmpX/OE8SBmmZgBCnVbwO/b90NkVDEYDawVGbEYfycfX74moakkm1zqB5P6qjacihFxwZXsudD8eDygbwvff/YKheeUGPPDwgJHqGGO3V+4b+bXiVCQnwFFZwKBgQDOd8xou+WXoCMf94N3aOcg1BRu5smHJs1bYRcubaxVPHFMa8LCOJ8XiS0iFqPdhxHgPdzNGT/n3DjZFV4P64jX6qNu+6N/fOojHSBj4FOVMq+nESblG3uyF1jrPu65xULOKgRwAjV8cPsOdWUzHTiOkZ/a0CgXseNiI57VAAS+ewKBgDWQaJtwcEOSYPSwRjOrGjAPlSkj/46MIMQb8ORfsCc6x6C4ftmVQ+Z6S8+ZXvS5MOXeU2ZH6yGoE6d0iomIhPIkvG5xjRjWoMmNxhJXgr5MugHZ7UdLQ0RYiHg4xquA4/G1J34KYJiymPX8zan/GIdkHnHFePbMJluxyxnlgGm1AoGBAJoEU79tKv/IvWsDQFa7Mm8SxYtVLdBb6aTY8Gn59iw/QmU3nbk0c7ki40Aik2qVb4hPnX6B72IOrXmCrwBBO3uV1QTdQkG/9QjsmVTn6nHJta5y5QjTT5qyP+p8r6h0tjkErvq/KxcBUMagXDWc/qubhhu8W6wRTwXOfJV3xhIxAoGAdB22F2D27iY55uz8VCQRzS4DehPx0eyipYy5KmT9MRv2FV9877tH/3wQ8amvHVBvJ/sQ4A7M1BcL/yUy1P6GD7DvM1KGXA1eybnCK+HINYr57A1rKA78kXPrXuogGYkUiy8LbMP3UvqV1q+BTlzJqfJqr20Fs6pjCEqoUu/uols=";
const PUBLIC: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAthJgkk8semFKTWUkSOtKJctFTdnGfWeCxcNwriusyMlXRVuJxkt0eDJeV+aXSHO5VZ9hTZqYto/9f0yhWcppvym+HIE5kYiJhdRnNjjvb+cZXBciRw1tCZfJC93xzWb8/vGowrV3FWtvRrqgkCHaD4CxGE3ws6oUkHOU++190A9fg6Dtp2N5F1O7ftRnlzIH1H3BCb9GD4xqKHyWTD/BszCYoyU1UyXPjNFoUSz5pulwq/RNJtzl2cTcp2M+nrhrTwJn5v5XeUEPhrvl2DJ2b7YQHZs8MvR8jL+HKrRVQfENiCTnpispoNQAzS0OOd4GTF0Q0UlBu5C7ANg3bLrKfQIDAQAB";
const PUBLIC_INFO: &str = "2.AQEBAQEBAQEBAQEBAQEBAQ==|105kcSEe0WvfYm7y6vSG0L9YzJuUfnQ0MpMAEY48xLzICztAD6U9cWZYk8C1y9wCnpu/KYYfxysgbGmipNBMi+V1LFOO0SZX58WA+xQFX8Ye6LR3+m8pfkVB8/9heJ7AcxCloIZVXfUWcENebBHzp7yix9KAKlZskh0xNALhUQt1HnphsVG1+nLqQmdWkzCsJzkLLRQfZA2ONmf8eH93816ojypkdjb5obAYXJBJlCBUMuPUInkHGflIUEBvsyCms7X5CAvjGWu4lYb98Cn4wg+OUWC6wglljFySZbdpiVQH7Fw1QXJQfBHb6RW7kCEqhAPpJ0XScREqHy7PXMlTdMLk7WAPWdA4qpoj8GNtOrKGYlWqqY2G7pb3lzhjW0+aowNOYlnVNy+SvtGVZmNKoXbfFyb2cFfda/32yTVmuSL0+5mt+BiytkqlYaBLf/fKjR7QMZ/RAqVUhb+HNqwGDPvBmjktMaFTjeiVebR/8YwJBmrGOcuDH+d0VHf0IhgpRT2DcU42bIUq8i0MnWM45vbktX0OIC2obI4fpjQFZbev1Zga198fo9HoKjJnHNdJDvFmvIdmWGcHzD1168zB0i6WYW2/CiODyo4GhpPgeg9ljM25S4ZPtZpuEUTfT6QKk2V/p97EW4O5zoaO2lVQHg==|ZuwHj1C6glbd0AiHJGlMJOXz7sNqsdtFb062Czhyq0w=";
const WRAPPED_KEY: &str = "4.EuvXROBPZ34R8seQ7rFXOtz/Xo2pA1J8M9imetJs/Hgzdo97H2b8zx/kDoGXwV5drx+MLjvxtrB97pwknDMV/RWYGlJQm+p+6Ov26J5czFjV2LlSA2eHHgf6ywbSAcV5jtF/Vfkrxfi2e5iGYMX5UIRPPYL1w2eRdIuL9zFe58tzc57BNFU2uleXaHJP9bD+W4xySd2etWpfXv5jONC9AGSBbKEDqOdF2E/vU0699O/gWTwni0eRdtwH7a3QfYnQ4J7WIBRXWZWFdbG2u7L6+hD52AEH02dWKuYEUc7FKqb9pudWk/KpJbPn5WnfPuRCVd85LA7jA8fNkGT5u11UZQ==";
const TEXT: &str = "2.AgICAgICAgICAgICAgICAg==|PVkyMjRh1LqLPdCvC2KprbgqIPEJF+dhb7RZkn044Mg=|ppcRW77HjPTJ2vCidsd72bWR315PQMtbHjh6JVO3218=";
const SENDER: &str = "2.AwMDAwMDAwMDAwMDAwMDAw==|ZTQFpiAQ3GVgjmpx4rslB5bEuieK2FqW2BezPCgJV8EyxvggJJKcHUp0+0vWvNpw|JriJ5+AwUwzO1VVBszG67fGfCdjv4/UMNXTeXlmaKjk=";
const FILE_NAME: &str = "2.BAQEBAQEBAQEBAQEBAQEBA==|WqmRkYp/kZRz6ENeCTG/qg==|veCi3L3qJ/TxTVkMIs+WANv1HRFBjd/vdktyH6YxppA=";
const FILE_KEY: &str = "2.BQUFBQUFBQUFBQUFBQUFBQ==|e3U8cRXwVERXx9IAKGeafY9xHsXioyNf1qysiZ6jGfFcJK+cBuWufPHw59iD9h2srmE2SeeC0AGrMv8/pYiHa3pUg4+t+pdxyGJHy2ckLbg=|Etld1BgHMz4c9MTlozjerPsmInl2tJFE8YVB+D8lePw=";
const FILE_DATA: &str = "AgYGBgYGBgYGBgYGBgYGBgZJseYrzVyRnatiIvHrdGCAeEZXhDwU46YbhR964PEj0fHSoX1XVxwEeoklx6yTI11UF4vyLjsgmbmKjnOFPjdi";
const USER_KEY: &str =
    "Pz49PDs6OTg3NjU0MzIxMC8uLSwrKikoJyYlJCMiISAfHh0cGxoZGBcWFRQTEhEQDw4NDAsKCQgHBgUEAwIBAA==";
const EXTRAS: &str =
    "wMHCw8TFxsfIycrLzM3Oz9DR0tPU1dbX2Nna29zd3t/g4eLj5OXm5+jp6uvs7e7v8PHy8/T19vf4+fr7/P3+/w==";
const USER_KEY_WRAPPED: &str = "2.CAgICAgICAgICAgICAgICA==|7eAzZaWHOkB1EkYjLtJnVZvi0glzzrOusO+nOqbAqVTMxbsHbYsyqfjYOPmbOP75eqoJ5d+0GNdqPA+YQzg3jWopXD1cnY+OuO1WWwh68nw=|pCeT+FTycPlEBwOFEVAvND0T+kSbSzqFWbScJlljRLw=";
/// HKDF-SHA256 over `PRIVATE`'s DER, salt `uwulock-extras-key-v1`, info
/// `private-key-wrap`, 64 bytes.
const PRIVATE_WRAP_KEY: &str =
    "I7W4NROnLfRjnxHjjsrXBSMf0VT12buENfrNG5x9x8idYmLcdW+G10GKnzB4sbvQIDzJiWWvs6IF0viZwdCiOQ==";
/// `EXTRAS` under `PRIVATE_WRAP_KEY`, IV 0x0b × 16.
const PRIVATE_KEY_WRAPPED: &str = "2.CwsLCwsLCwsLCwsLCwsLCw==|dGJllzHWdkm1gtGx95al1qVqRbqeAPZ9OKyq/MxXLfGqJMxKQ+poatuIIrCcU2xZY0MErNR2ZjPdjzUEo8uyAVFKgF7JjotkYjc4qEaEOmA=|NgbzJL5yOpHAO3ean8EFV/2VTjKEUEL6aQx3oB423jo=";
/// HMAC-SHA256 under `EXTRAS` over `uwulock-extras-key-id-v1`, 16 bytes.
const KEY_ID: &str = "14714c070a5c5b6d374438ed28849f08";
/// Another account's key pair (RSA-1024, only to be another one).
const OTHER_PRIVATE: &str = "MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAK97Tqp336NvtZYtBTUPt8TYMq6+jMntikTj9+s2tnT+vVt8EX+6GDH8jkN6E1wLbrHp2Qy5qxEmMxiE8rX6NkRMpWLhTJc128QA+MC5k929V4cId/luNh2piCw5O4bL4pINj5MJbDCvNNLrirNA/NkryjUU4vOWRsNrc32x0RqNAgMBAAECgYBwBRsWnydYQbt9fofQc5QwSIMyIdnmHYkiqRReRrL6xJNEj1LsYnOHlV2LnaY2H+YuFMXF5dBaRjRf9p6ppGx25c0kz0eVG7o87LyG0xty313GL6dn0MQpYmmSbbONdQrdyYK/aue71nBOHe1qXSl84FgmTLkFL9fYneZARrQygQJBAORFaMQUHEjlZZiVS/jZgSv9anuKJTsosKLo3qc5cGE78JFL/sc+yH0Cmyv6xQVDc9kO7C25NovVS121yJQYwZkCQQDEzEtM2XwBvzgbuBk8wtfD8Objo73gMxUEWjNcRSJIuXXdrWKu9MztnK/wWMh/y1leDtTau64nXkCHvS4fSKEVAkACcQ+e0UxAJ1v/1tD6N3FfRBWofqDJUjUZeP4wsbeXAqofE74E6ZIBbE62mLcUyFTr5HH4RzvjIQPuW6xqkR05AkBkU0mn+c9wDI2MBARJp4LbjvoF3rmzjBcQyvMX/N6HeJSP2A5Q5td54sEGpBxCmeYLP0Bf6gHUbAY1rMnQhPQpAkEApSSl85X17/XhfTqOLcMekigcAqD0Iwlp5ykCAJKgWpNGryb/PC0RRQcdAlZYucJTkavFxup2THe8tKxhQpGknA==";
const SPACE_KEY: &str = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk=";
const SPACE_KEY_WRAPPED: &str = "2.CgoKCgoKCgoKCgoKCgoKCg==|3x90kprUnecJIwTVZjj35QCgjH56EAkqnN2Y2xU1oxuJVNKmuQL3HpwyItmtEGI7|4v3NsUii/4UB3N2TnGubjpBhGY/JrPNuPJWe9IWyy3M=";

fn key(base64: &str) -> SymmetricKey {
    SymmetricKey::from_bytes(&B64.decode(base64).unwrap()).unwrap()
}

fn private() -> PrivateKey {
    PrivateKey::from_der(&B64.decode(PRIVATE).unwrap()).unwrap()
}

fn other_private() -> PrivateKey {
    PrivateKey::from_der(&B64.decode(OTHER_PRIVATE).unwrap()).unwrap()
}

fn secret() -> LinkSecret {
    LinkSecret::from_bytes(&B64.decode(SECRET).unwrap()).unwrap()
}

#[test]
fn the_link_key_is_hkdf_with_bitwardens_salt() {
    assert_eq!(B64.encode(secret().key().to_bytes()), LINK_KEY);
}

#[test]
fn opens_public_details_made_elsewhere() {
    let info = PublicInfo::open(PUBLIC_INFO, &secret()).unwrap();
    assert_eq!(info.title, "Passport scan");
    assert_eq!(info.note.as_deref(), Some("Please upload both pages."));
    assert_eq!(info.owner.as_deref(), Some("Lorin"));
    assert_eq!(info.public_key, PUBLIC);
    assert_eq!(info.public_key().unwrap(), private().public());
    // The owner's check: the details name their own key, not another.
    assert!(info.is_for(&private().public()));
    assert!(!info.is_for(&other_private().public()));
    let broken = PublicInfo {
        public_key: "not a key".into(),
        ..info
    };
    assert!(!broken.is_for(&private().public()));
    // Another link's secret doesn't open it.
    let other = LinkSecret::from_bytes(&[1; 16]).unwrap();
    assert!(PublicInfo::open(PUBLIC_INFO, &other).is_err());
}

#[test]
fn opens_a_submission_made_elsewhere() {
    let k = SubmissionKey::open(WRAPPED_KEY, &private()).unwrap();
    assert_eq!(k.open_text(TEXT).unwrap().as_str(), "Both pages attached.");
    assert_eq!(
        k.open_sender(SENDER).unwrap(),
        Sender {
            name: Some("Ada".into()),
            email: Some("ada@example.com".into())
        }
    );
    let (name, file_key) = k
        .open_file(&SealedFile {
            file_name: FILE_NAME.into(),
            key: FILE_KEY.into(),
        })
        .unwrap();
    assert_eq!(name.as_str(), "passport.pdf");
    let data = B64.decode(FILE_DATA).unwrap();
    assert_eq!(
        file_key.decrypt(&data).unwrap().as_slice(),
        b"%PDF-1.7 passport scan"
    );

    // Taken into an item: the same bytes open with the key re-wrapped under
    // the item's key, as an attachment's does.
    let item_key = SymmetricKey::generate();
    let moved = file_key.for_item("passport.pdf", &item_key);
    let attachment_key = moved
        .key
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&item_key)
        .unwrap();
    assert_eq!(
        decrypt_file(&data, &attachment_key).unwrap().as_slice(),
        b"%PDF-1.7 passport scan"
    );
    assert_eq!(
        moved
            .file_name
            .parse::<EncString>()
            .unwrap()
            .decrypt_string(&item_key)
            .unwrap()
            .as_str(),
        "passport.pdf"
    );
}

#[test]
fn a_whole_file_request_round_trip() {
    let owner = private();
    let secret = LinkSecret::generate();
    let sealed = PublicInfo::new("Tax papers", None, Some("Lorin"), &owner.public())
        .unwrap()
        .seal(&secret)
        .unwrap();

    // The uploader's page: only the link's secret and the public details.
    let info = PublicInfo::open(
        &sealed,
        &LinkSecret::from_link_part(&secret.to_link_part()).unwrap(),
    )
    .unwrap();
    let k = SubmissionKey::generate();
    let wrapped = k.wrap(&info.public_key().unwrap()).unwrap();
    assert!(wrapped.starts_with("4."));
    let text = k.seal_text("hello").unwrap();
    let (file_key, file) = k.new_file("a.txt");
    let data = file_key.encrypt(b"contents");
    assert!(k.seal_text(&"x".repeat(100_001)).is_err());

    // The owner.
    let k = SubmissionKey::open(&wrapped, &owner).unwrap();
    assert_eq!(k.open_text(&text).unwrap().as_str(), "hello");
    let (name, file_key) = k.open_file(&file).unwrap();
    assert_eq!(name.as_str(), "a.txt");
    assert_eq!(file_key.decrypt(&data).unwrap().as_slice(), b"contents");
}

#[test]
fn the_private_wrap_key_is_hkdf_over_the_private_keys_der() {
    // The DER the key is derived from is the one `to_der` writes; for a key
    // made elsewhere it is the same bytes.
    assert_eq!(B64.encode(private().to_der().unwrap()), PRIVATE);
    assert_eq!(
        B64.encode(extras::private_wrap_key(&private()).unwrap().to_bytes()),
        PRIVATE_WRAP_KEY
    );
    assert_eq!(extras::key_id(&key(EXTRAS)), KEY_ID);
}

fn wrapped(user: Option<&str>, private: Option<&str>) -> Keys {
    Keys {
        extras_key: Some(WrappedExtrasKey {
            user_key_wrapped: user.map(str::to_string),
            private_key_wrapped: private.map(str::to_string),
            revision_date: None,
        }),
        lost: false,
    }
}

#[test]
fn opens_the_extras_key_under_the_user_key_and_checks_the_other_wrap() {
    let keys = wrapped(Some(USER_KEY_WRAPPED), Some(PRIVATE_KEY_WRAPPED));
    for private in [None, Some(private())] {
        match resolve(&keys, &key(USER_KEY), private.as_ref()).unwrap() {
            Resolved::Open {
                key: opened,
                rewrap,
                private_wrap,
            } => {
                assert_eq!(B64.encode(opened.to_bytes()), EXTRAS);
                assert!(rewrap.is_none() && private_wrap.is_none());
            }
            other => panic!("{other:?}"),
        }
    }
    // Wraps of two different keys: the server changed one.
    let bound = extras::private_wrap_key(&private()).unwrap();
    let other = EncString::encrypt(&SymmetricKey::generate().to_bytes(), &bound).to_string();
    let keys = wrapped(Some(USER_KEY_WRAPPED), Some(&other));
    assert!(resolve(&keys, &key(USER_KEY), Some(&private())).is_err());
    // A private wrap made for another account's key doesn't check either.
    let theirs = extras::private_wrap_key(&other_private()).unwrap();
    let keys = wrapped(
        Some(USER_KEY_WRAPPED),
        Some(&EncString::encrypt(&key(EXTRAS).to_bytes(), &theirs).to_string()),
    );
    assert!(resolve(&keys, &key(USER_KEY), Some(&private())).is_err());
}

#[test]
fn a_key_from_before_the_private_wrap_gets_one() {
    let keys = wrapped(Some(USER_KEY_WRAPPED), None);
    let Resolved::Open { private_wrap, .. } = resolve(&keys, &key(USER_KEY), None).unwrap() else {
        panic!("not opened");
    };
    assert!(private_wrap.is_none(), "nothing to wrap it with");
    let Resolved::Open {
        key: opened,
        rewrap,
        private_wrap,
    } = resolve(&keys, &key(USER_KEY), Some(&private())).unwrap()
    else {
        panic!("not opened");
    };
    assert!(rewrap.is_none());
    let private_wrap = private_wrap.expect("a private wrap to add");
    assert_eq!(
        serde_json::to_value(&private_wrap).unwrap(),
        json!({ "privateKeyWrapped": private_wrap.private_key_wrapped })
    );
    let again = private_wrap
        .private_key_wrapped
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&key(PRIVATE_WRAP_KEY))
        .unwrap();
    assert_eq!(again.to_bytes(), opened.to_bytes());
}

#[test]
fn after_an_official_rotation_the_private_key_opens_it_and_it_is_wrapped_again() {
    let new_user_key = SymmetricKey::generate();
    let keys: Keys = serde_json::from_value(json!({
        "object": "uwuKeys",
        "extrasKey": { "userKeyWrapped": null, "privateKeyWrapped": PRIVATE_KEY_WRAPPED, "revisionDate": "2026-09-28T12:00:00.000000Z" },
        "lost": false
    }))
    .unwrap();
    // Without the private key, this client can't: nothing is written.
    assert!(matches!(
        resolve(&keys, &new_user_key, None).unwrap(),
        Resolved::Lost
    ));
    // Nor with another account's.
    assert!(resolve(&keys, &new_user_key, Some(&other_private())).is_err());
    let Resolved::Open {
        key: opened,
        rewrap,
        private_wrap,
    } = resolve(&keys, &new_user_key, Some(&private())).unwrap()
    else {
        panic!("not opened");
    };
    assert_eq!(B64.encode(opened.to_bytes()), EXTRAS);
    assert!(private_wrap.is_none());
    let rewrap = rewrap.expect("a new user wrap");
    let again = rewrap
        .user_key_wrapped
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&new_user_key)
        .unwrap();
    assert_eq!(B64.encode(again.to_bytes()), EXTRAS);
    assert_eq!(
        serde_json::to_value(&rewrap).unwrap(),
        json!({ "userKeyWrapped": rewrap.user_key_wrapped })
    );
}

#[test]
fn an_rsa_wrap_the_server_could_have_made_is_never_taken() {
    // Anyone with the account's public key — the server — can make this.
    let chosen = SymmetricKey::generate();
    let rsa = wrap_for(&private().public(), &chosen).unwrap().to_string();
    let user_key = SymmetricKey::generate();
    // 0.3's beta field is not read at all: nothing left that opens.
    let keys: Keys = serde_json::from_value(json!({
        "extrasKey": { "userKeyWrapped": null, "publicKeyWrapped": rsa },
        "lost": false
    }))
    .unwrap();
    assert!(matches!(
        resolve(&keys, &user_key, Some(&private())).unwrap(),
        Resolved::Lost
    ));
    // Nor in the new field, beside a user wrap or without one.
    for user in [None, Some(USER_KEY_WRAPPED)] {
        let keys = wrapped(user, Some(&rsa));
        let error = resolve(&keys, &key(USER_KEY), Some(&private())).unwrap_err();
        assert!(error.to_string().contains("private key"), "{error}");
    }
}

#[test]
fn a_new_extras_key_opens_both_ways_and_lost_stays_lost() {
    let user_key = SymmetricKey::generate();
    let owner = private();
    let Resolved::Create(made) = resolve(&Keys::default(), &user_key, Some(&owner)).unwrap() else {
        panic!("nothing to create");
    };
    let body = serde_json::to_value(&made.request).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 2);
    assert!(body["userKeyWrapped"].as_str().unwrap().starts_with("2."));
    assert!(body["privateKeyWrapped"]
        .as_str()
        .unwrap()
        .starts_with("2."));
    let by_private = made
        .request
        .private_key_wrapped
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&key(PRIVATE_WRAP_KEY))
        .unwrap();
    assert_eq!(by_private.to_bytes(), made.key.to_bytes());
    // Wrapped again for a rotation of UwULock's own.
    let next = SymmetricKey::generate();
    let rotated = extras::wrap(&made.key, &next, &owner).unwrap();
    let keys = wrapped(
        Some(&rotated.user_key_wrapped),
        Some(&rotated.private_key_wrapped),
    );
    assert!(matches!(
        resolve(&keys, &next, Some(&owner)).unwrap(),
        Resolved::Open {
            rewrap: None,
            private_wrap: None,
            ..
        }
    ));

    let lost = Keys {
        extras_key: None,
        lost: true,
    };
    assert!(matches!(
        resolve(&lost, &user_key, Some(&owner)).unwrap(),
        Resolved::Lost
    ));
    // Nothing to wrap a new key for without the key pair.
    assert!(resolve(&Keys::default(), &user_key, None).is_err());
}

#[test]
fn opens_a_space_key_made_elsewhere() {
    let space = SpaceKey::unwrap(SPACE_KEY_WRAPPED, &key(EXTRAS)).unwrap();
    assert_eq!(B64.encode(space.as_bytes()), SPACE_KEY);
    let fresh = SpaceKey::generate();
    let back = SpaceKey::unwrap(&fresh.wrap(&key(EXTRAS)), &key(EXTRAS)).unwrap();
    assert_eq!(back.as_bytes(), fresh.as_bytes());
    assert!(SpaceKey::unwrap(SPACE_KEY_WRAPPED, &SymmetricKey::generate()).is_err());
}

/// A 1 × 1 PNG, and one that claims 129 × 1.
fn png(width: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    png
}

#[test]
fn own_icons_are_small_pngs_under_their_key() {
    let extras = key(EXTRAS);
    let sealed = seal_icon(&png(1), &extras).unwrap();
    assert_eq!(open_icon(&sealed, &extras).unwrap().as_slice(), png(1));
    assert!(seal_icon(&png(129), &extras).is_err());
    assert!(seal_icon(b"GIF89a", &extras).is_err());
    let mut big = png(128);
    big.resize(80 * 1024, 0);
    assert!(seal_icon(&big, &extras).is_err());
}

#[test]
fn a_version_is_encrypted_again_for_a_new_user_key() {
    let old = key(USER_KEY);
    let new = SymmetricKey::generate();
    let seal = |text: &str| EncString::encrypt(text.as_bytes(), &old).to_string();
    let open = |value: &serde_json::Value, key: &SymmetricKey| {
        value
            .as_str()
            .unwrap()
            .parse::<EncString>()
            .unwrap()
            .decrypt_string(key)
            .unwrap()
            .to_string()
    };

    // Without an item key: every value.
    let version = json!({
        "type": 1, "name": seal("Shop"), "notes": null, "key": null,
        "login": { "username": seal("nyu"), "password": seal("old"), "passwordRevisionDate": "2026-09-01T10:00:00.000Z",
                   "uris": [{ "uri": seal("https://shop.example.com"), "match": null }] },
        "fields": [{ "name": seal("PIN"), "value": seal("1234"), "type": 1 }],
        "passwordHistory": [{ "password": seal("older"), "lastUsedDate": "2026-08-01T10:00:00.000Z" }],
        "reprompt": 0
    });
    let again = reencrypt_version(&version, &old, &new).unwrap();
    assert_eq!(open(&again["name"], &new), "Shop");
    assert_eq!(open(&again["login"]["password"], &new), "old");
    assert_eq!(
        open(&again["login"]["uris"][0]["uri"], &new),
        "https://shop.example.com"
    );
    assert_eq!(open(&again["fields"][0]["value"], &new), "1234");
    assert_eq!(
        open(&again["passwordHistory"][0]["password"], &new),
        "older"
    );
    assert_eq!(
        again["login"]["passwordRevisionDate"],
        "2026-09-01T10:00:00.000Z"
    );

    // With one: only the item key is wrapped again.
    let item_key = SymmetricKey::generate();
    let name = EncString::encrypt(b"Bank", &item_key).to_string();
    let version = json!({
        "type": 1, "name": name, "key": EncString::encrypt(&item_key.to_bytes(), &old).to_string()
    });
    let again = reencrypt_version(&version, &old, &new).unwrap();
    assert_eq!(again["name"], version["name"]);
    let rewrapped = again["key"]
        .as_str()
        .unwrap()
        .parse::<EncString>()
        .unwrap()
        .decrypt_key(&new)
        .unwrap();
    assert_eq!(rewrapped.to_bytes(), item_key.to_bytes());

    // A value the old key doesn't open is an error, not dropped.
    let broken =
        json!({ "type": 2, "name": EncString::encrypt(b"x", &new).to_string(), "key": null });
    assert!(reencrypt_version(&broken, &old, &new).is_err());
}

#[test]
fn a_public_key_parses_as_the_server_hands_it_out() {
    let public = PublicKey::from_der(&B64.decode(PUBLIC).unwrap()).unwrap();
    assert_eq!(public, private().public());
}
