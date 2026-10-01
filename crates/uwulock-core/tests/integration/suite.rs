//! The suite vault (contract §6) against UwUSSH and UwURDP themselves: the
//! `APP_*` records were sealed once by `uwussh-vault`'s and `uwurdp-vault`'s
//! `encrypt_synced`, and the `CORE_*` blobs (sealed here with a fixed nonce)
//! were opened once by their `decrypt_synced`. If these hold, what the web
//! vault and UwULock's apps write opens in the apps, and the other way round.

use serde_json::json;
use uuid::Uuid;
use uwulock_core::crypto::SymmetricKey;
use uwulock_core::extras::SpaceKey;
use uwulock_core::suite::openssh::{
    generate_ed25519, inspect_private_key, inspect_public_key, passphrase_opens,
};
use uwulock_core::suite::{
    associated_data, Envelope, Hlc, Payload, RecordHeader, Space, SpaceVault, SuiteSpace,
};
use uwulock_core::Error;

const KEY: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];
const SPACE_ID: &str = "3f0e8a6c-1b2d-4e5f-8a9b-0c1d2e3f4a5b";
const HOST_ID: &str = "9b2d0c4e-5f60-4a1b-8c2d-3e4f5a6b7c8d";
const SECRET_ID: &str = "0a1b2c3d-4e5f-4a6b-9c7d-8e9fa0b1c2d3";
const ASSIST_ID: &str = "11111111-2222-4333-8444-555555555555";
const RDP_HOST_ID: &str = "7c8d9e0f-1a2b-4c3d-8e4f-5a6b7c8d9e0f";
const SSH_HOST: &str = r#"{"name":"prod","address":"203.0.113.10","port":2222,"workspace":"private","position":3,"group_id":null,"identity_id":null,"future_field":{"x":1}}"#;
const RDP_HOST: &str = r#"{"name":"desk","address":"192.0.2.7","port":3389,"nla":true}"#;

/// One record of the vectors: space, id, kind, clock, deleted, plaintext,
/// the app's nonce and blob, the blob sealed here under nonce `[0x40 + n; 24]`.
struct Vector {
    space: Space,
    id: &'static str,
    kind: &'static str,
    clock: (u64, u32, u32),
    deleted: bool,
    plain: &'static [u8],
    app_nonce: &'static str,
    app_blob: &'static str,
    core_blob: &'static str,
}

const VECTORS: [Vector; 6] = [
    Vector {
        space: Space::Ssh,
        id: HOST_ID,
        kind: "host",
        clock: (1_790_000_000_000, 0, 305_419_896),
        deleted: false,
        plain: SSH_HOST.as_bytes(),
        app_nonce: "YHGzMhRENPZi94GToUenZYXokQ7uCnVv",
        app_blob: "MspZOzcD4cRb7oJEvnHr/lqcFo8vhojjzLbpcJCAZUCpctQ4GHDBzrDWV/x3oBbf7Yt2XoLXwvXaU9fTUvwnNsbcKelE8NkgX0yKusUyIN1S0BZzwe8otdQOMaM5ugaroaXHHfBf0BH7K09a6rfMQOGa7jNIJi1Oxd4VmDkxA+Ij0gAoFYu09uY39WB8xyauyVt27IdiKwkL8pxpb2OHyx4=",
        core_blob: "eCoOCEn+xfGYFlZKBgN6CHQMujqqzQelwPqRvc+QqhG+92Jb7AIyqN3q/TSax0fOv0YkBo/p/fASGtamghgXrE73wNcgTSBz0Pl+Jy6yCFFtIiLc/HMOzIqeRoPWV6lHiorjkX77KIEbd4JnpfFDZCfxdwfOzcFCzLoq5KACkOij5pyt8hV1G3npHtYxF6xxPL89UcT543EO7YuiK8CFehs=",
    },
    Vector {
        space: Space::Ssh,
        id: SECRET_ID,
        kind: "secret",
        clock: (1_790_000_000_001, 7, 42),
        deleted: false,
        plain: "hunter2 ä".as_bytes(),
        app_nonce: "PKvzbmTNvBLl8M5xZuGQfMM/0nWzftby",
        app_blob: "ieOtIrIO7t7KCJUtYBs7QIBkFXAnj5Md5YM=",
        core_blob: "goNYlZLIcroKyZ/h0rjnyv4Y7SG+qYE5E3o=",
    },
    Vector {
        space: Space::Ssh,
        id: HOST_ID,
        kind: "host",
        clock: (1_790_000_000_500, 1, 305_419_896),
        deleted: true,
        plain: b"",
        app_nonce: "SutOU6ay2LttAnacczBjDwaiTfLy7b2c",
        app_blob: "6c4+1mHp5xcn58Q4fQUc2A==",
        core_blob: "ysPXyW/QQWQ9wbw1dRvSKA==",
    },
    Vector {
        space: Space::Ssh,
        id: ASSIST_ID,
        kind: "assist_config",
        clock: (1_790_000_000_002, 0, 9),
        deleted: false,
        plain: br#"{"provider":"ollama"}"#,
        app_nonce: "x1NJ7hWRVdD5magIvWjBR7xzFUhfwLp+",
        app_blob: "JGApOoFXmb++g9JL4hSNPnlAVr/vNZi307KgJPTEX/k6M6jDsA==",
        core_blob: "LBna38Ov3Pqy/EZqhjmLm9nbdUOctD+ZzzMFNHpw3f5tMzeWgQ==",
    },
    Vector {
        space: Space::Rdp,
        id: RDP_HOST_ID,
        kind: "host",
        clock: (1_790_000_001_000, 2, 3_000_000_000),
        deleted: false,
        plain: RDP_HOST.as_bytes(),
        app_nonce: "IbjTZKLL+FzcGvFwy5reHGnmNMLsIoHY",
        app_blob: "ZUpht0uElY5kI/Wtl9MZh50cTmb5awSRbXE5s1OV2ih903eQOxiBRJvOgEqkjV5LBahSuxvyVgknjqZD9u0r2oxxZ+YAU2lT26/djw==",
        core_blob: "97Y6CazbWkFmrtqLhiMaVKOFkdnhutktpXRMLZ+SyiBt9hv3SHYqvxCRxdmQM+o2vjW5z8Pdi7aiuAEU5GkzNLsPVEy5lAFd7sqehg==",
    },
    Vector {
        space: Space::Rdp,
        id: RDP_HOST_ID,
        kind: "host",
        clock: (1_790_000_002_000, 0, 3_000_000_000),
        deleted: true,
        plain: b"",
        app_nonce: "nQFSxIoQN95vcZMwNmbovcr15Eno2GEZ",
        app_blob: "uxIaFvuHsMFzk5CSdcNKrA==",
        core_blob: "DrqqjlD7Veik/jo7+V0Qaw==",
    },
];

fn vault(space: Space) -> SpaceVault {
    SpaceVault::new(
        space,
        Uuid::parse_str(SPACE_ID).unwrap(),
        SpaceKey::from_bytes(&KEY).unwrap(),
    )
}

fn app_envelope(n: usize) -> Envelope {
    let v = &VECTORS[n];
    Envelope {
        id: v.id.into(),
        kind: v.kind.into(),
        updated_at: Hlc::new(v.clock.0, v.clock.1, v.clock.2),
        base_seq: 0,
        deleted: v.deleted,
        nonce: v.app_nonce.into(),
        blob: v.app_blob.into(),
        seq: Some(10 + n as u64),
    }
}

#[test]
fn records_the_apps_sealed_open_here() {
    for (n, v) in VECTORS.iter().enumerate() {
        let opened = vault(v.space).open(&app_envelope(n)).unwrap();
        assert_eq!(&opened[..], v.plain, "vector {n}");
    }
}

#[test]
fn records_sealed_here_are_what_the_apps_opened() {
    for (n, v) in VECTORS.iter().enumerate() {
        let header = RecordHeader {
            id: Uuid::parse_str(v.id).unwrap(),
            kind: v.kind.into(),
            updated_at: Hlc::new(v.clock.0, v.clock.1, v.clock.2),
            deleted: v.deleted,
            base_seq: 0,
        };
        let sealed = vault(v.space)
            .seal_with_nonce(&header, v.plain, [0x40 + n as u8; 24])
            .unwrap();
        assert_eq!(sealed.blob, v.core_blob, "vector {n}");
        assert_eq!(&vault(v.space).open(&sealed).unwrap()[..], v.plain);
    }
}

#[test]
fn associated_data_is_the_contracts() {
    let id = Uuid::parse_str(HOST_ID).unwrap();
    let space = Uuid::parse_str(SPACE_ID).unwrap();
    let aad = associated_data(
        Space::Ssh,
        &id,
        5,
        &space,
        Hlc::new(0x0102_0304_0506_0708, 0x0a0b_0c0d, 0x1112_1314),
        true,
    );
    let mut expected = b"uwussh/record/v2".to_vec();
    expected.extend_from_slice(id.as_bytes());
    expected.push(5);
    expected.extend_from_slice(space.as_bytes());
    expected.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0x0a, 0x0b, 0x0c, 0x0d]);
    expected.extend_from_slice(&[0x11, 0x12, 0x13, 0x14, 1]);
    assert_eq!(aad, expected);
    assert_eq!(aad.len(), 16 + 16 + 1 + 16 + 16 + 1);

    let rdp = associated_data(Space::Rdp, &id, 0, &space, Hlc::default(), false);
    assert!(rdp.starts_with(b"uwurdp/record/v2"));
    for other in [Space::Mail, Space::Generic] {
        let aad = associated_data(other, &id, 0, &space, Hlc::default(), false);
        assert!(aad.starts_with(b"uwulock/suite/v1"));
    }
}

#[test]
fn a_changed_header_or_another_space_does_not_open() {
    let ssh = vault(Space::Ssh);
    let good = app_envelope(0);
    let mut deleted = good.clone();
    deleted.deleted = true;
    let mut older = good.clone();
    older.updated_at.wall_ms -= 1;
    let mut moved = good.clone();
    moved.id = SECRET_ID.into();
    for bad in [deleted, older, moved] {
        assert!(ssh.open(&bad).is_err());
    }
    // The same key and id, but UwURDP's prefix.
    assert!(vault(Space::Rdp).open(&good).is_err());
    // Another space id.
    let elsewhere = SpaceVault::new(
        Space::Ssh,
        Uuid::from_u128(1),
        SpaceKey::from_bytes(&KEY).unwrap(),
    );
    assert!(elsewhere.open(&good).is_err());
    // A kind this build doesn't know is passed on, not opened.
    let mut unknown = good;
    unknown.kind = "teleporter".into();
    assert!(matches!(ssh.open(&unknown), Err(Error::Unsupported(_))));
}

#[test]
fn kinds_per_space() {
    assert_eq!(Space::Ssh.kind_discriminant("assist_config"), Some(10));
    assert_eq!(Space::Ssh.kind_discriminant("assist_cache"), Some(11));
    assert_eq!(Space::Ssh.kind_name(5), Some("port_forward"));
    assert_eq!(Space::Rdp.kind_discriminant("assist_config"), None);
    assert_eq!(Space::Rdp.kind_discriminant("manifest"), Some(9));
    assert_eq!(Space::Mail.kind_discriminant("account"), Some(0));
    assert_eq!(Space::Generic.kind_name(0), Some("item"));
    assert_eq!(Space::Generic.kind_name(1), None);
    assert_eq!("rdp".parse::<Space>().unwrap(), Space::Rdp);
    assert!("ftp".parse::<Space>().is_err());
    assert_eq!(serde_json::to_value(Space::Ssh).unwrap(), json!("ssh"));
}

#[test]
fn the_clock_never_runs_back() {
    let a = Hlc::new(1_000, 0, 7);
    assert_eq!(a.tick(1_000), Hlc::new(1_000, 1, 7));
    assert_eq!(Hlc::new(1_000, 5, 7).tick(1_001), Hlc::new(1_001, 0, 7));
    assert!(Hlc::new(5_000, 0, 7).tick(4_000) > Hlc::new(5_000, 0, 7));
    let full = Hlc::new(1_000, u32::MAX, 7);
    assert_eq!(full.tick(1_000), Hlc::new(1_001, 0, 7));
    // An edit of a record another device wrote "in the future", with a
    // larger device id: still after it, with this device's id.
    let theirs = Hlc::new(9_000, 3, u32::MAX);
    let mine = Hlc::after(theirs, 1_000, 1);
    assert_eq!(mine, Hlc::new(9_000, 4, 1));
    assert!(mine > theirs);
    assert_eq!(Hlc::after(theirs, 9_001, 1), Hlc::new(9_001, 0, 1));
}

#[test]
fn an_editor_opens_changes_and_seals() {
    let ssh = vault(Space::Ssh);
    let host = ssh.open_record(&app_envelope(0)).unwrap();
    assert_eq!(host.seq, 10);
    let Some(Payload::Json(mut value)) = host.payload.clone() else {
        panic!("a host is JSON: {:?}", host.payload);
    };
    assert_eq!(value["future_field"], json!({ "x": 1 }));
    value["port"] = json!(22);

    let edit = ssh
        .seal_edit(&host.head(), &Payload::Json(value), 1_000, 77)
        .unwrap();
    assert_eq!(edit.id, HOST_ID);
    assert_eq!(edit.base_seq, 10);
    assert!(edit.updated_at > host.updated_at);
    assert_eq!(edit.updated_at.device, 77);
    assert_eq!(edit.seq, None);
    let again = ssh.open_record(&edit).unwrap();
    let Some(Payload::Json(value)) = again.payload else {
        panic!()
    };
    assert_eq!(value["port"], json!(22));
    assert_eq!(
        value["future_field"],
        json!({ "x": 1 }),
        "unknown fields stay"
    );

    let secret = ssh.open_record(&app_envelope(1)).unwrap();
    assert_eq!(secret.payload, Some(Payload::Text("hunter2 ä".into())));
    let tomb = ssh.open_record(&app_envelope(2)).unwrap();
    assert!(tomb.deleted && tomb.payload.is_none());

    let new = ssh
        .seal_new("secret", &Payload::Text("s3cret".into()), 2_000, 77)
        .unwrap();
    assert_eq!(new.base_seq, 0);
    assert_eq!(new.updated_at, Hlc::new(2_000, 0, 77));
    let id = Uuid::parse_str(&new.id).unwrap();
    assert_eq!(id.get_version_num(), 4);
    assert_eq!(
        ssh.open_record(&new).unwrap().payload,
        Some(Payload::Text("s3cret".into()))
    );
    let bytes = ssh
        .seal_new("secret", &Payload::Bytes(vec![0xff, 0]), 2_000, 77)
        .unwrap();
    assert_eq!(
        ssh.open_record(&bytes).unwrap().payload,
        Some(Payload::Bytes(vec![0xff, 0]))
    );

    let gone = ssh.seal_tombstone(&host.head(), 1_000, 77).unwrap();
    assert!(gone.deleted);
    assert_eq!(gone.base_seq, 10);
    assert!(ssh.open(&gone).unwrap().is_empty());
    assert_eq!(gone.head().unwrap().id, host.id);

    // What isn't written here.
    let text = Payload::Text("x".into());
    assert!(ssh.seal_new("host", &text, 1, 1).is_err());
    assert!(ssh
        .seal_new("secret", &Payload::Json(json!({})), 1, 1)
        .is_err());
    assert!(ssh
        .seal_new("manifest", &Payload::Json(json!({})), 1, 1)
        .is_err());
    assert!(ssh
        .seal_new("teleporter", &Payload::Json(json!({})), 1, 1)
        .is_err());
    let mut manifest = host.head();
    manifest.kind = "manifest".into();
    assert!(ssh.seal_tombstone(&manifest, 1, 1).is_err());
}

#[test]
fn wire_json_is_the_contracts() {
    let text = r#"{ "id": "9b2d", "kind": "host",
        "updatedAt": { "wallMs": 1790000000000, "counter": 0, "device": 305419896 },
        "baseSeq": 0, "deleted": false, "nonce": "AAAA", "blob": "BBBB", "seq": 17 }"#;
    let envelope: Envelope = serde_json::from_str(text).unwrap();
    assert_eq!(envelope.seq, Some(17));
    assert_eq!(envelope.updated_at.device, 305_419_896);
    let fresh = Envelope {
        seq: None,
        ..envelope
    };
    let out = serde_json::to_value(&fresh).unwrap();
    assert!(out.get("seq").is_none());
    assert_eq!(out["updatedAt"]["wallMs"], json!(1_790_000_000_000u64));

    let push = vault(Space::Ssh).push_request(vec![fresh]);
    let push = serde_json::to_value(push).unwrap();
    assert_eq!(push["schema"], json!(2));
    assert_eq!(push["spaceId"], json!(SPACE_ID));
    assert_eq!(push["records"][0]["baseSeq"], json!(0));

    assert_eq!(
        serde_json::to_value(Payload::Bytes(vec![1, 2, 3])).unwrap(),
        json!({ "bytes": "AQID" })
    );
    assert_eq!(
        serde_json::from_value::<Payload>(json!({ "text": "pw" })).unwrap(),
        Payload::Text("pw".into())
    );
    assert_eq!(
        serde_json::to_value(Payload::Json(json!({ "a": 1 }))).unwrap(),
        json!({ "json": { "a": 1 } })
    );
}

#[test]
fn a_space_made_here_opens_from_the_list() {
    let extras = SymmetricKey::generate();
    let (made, request) = SpaceVault::create(Space::Rdp, &extras);
    let listed = SuiteSpace {
        space: "rdp".into(),
        id: request.id.clone(),
        key: request.key.clone(),
        ..Default::default()
    };
    let opened = SpaceVault::open_space(&listed, &extras).unwrap();
    assert_eq!(opened.space, Space::Rdp);
    assert_eq!(opened.id, made.id);
    assert_eq!(opened.key().as_bytes(), made.key().as_bytes());
    let record = made
        .seal_new("host", &Payload::Json(json!({ "name": "a" })), 1, 2)
        .unwrap();
    assert!(opened.open(&record).is_ok());
}

/// `ssh-keygen -t ed25519 -a 2 -N 'correct horse' -C test@example.com`.
const KEYGEN_PRIVATE: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABCc0H+P4a
iiWFWGbpHLXINZAAAAAgAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAICy8QmzGmnmnT24J
YEFz1E+RO9DB9HXiPBrpznIumPY7AAAAoCqII3ZCE5ZfbTpQy/i7pzaB9ixcHK2VnmYw70
b4A4SmvRwwTtZCst+YL1cSo9lo5o5V8qJn7utOTORG+FiYKnDCG/senFMxUhCUhzpvgRMQ
yCh0wGKZ+5fGNvCQ1aUyNg1/ao0/lyUzQU2bKm9VqJ1/jjq0Ag438l5wWZoo8MFU1i5N+m
4xf0Qd1PTXtG3NnQk5rwsYqYuD7psW8KkS9eY=
-----END OPENSSH PRIVATE KEY-----
";
const KEYGEN_PUBLIC: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICy8QmzGmnmnT24JYEFz1E+RO9DB9HXiPBrpznIumPY7 test@example.com";
const KEYGEN_FINGERPRINT: &str = "SHA256:/OXMM8lupQ1qoQJRRM+5ed3WQfwOKzEljTLqpMGVuz8";

#[test]
fn keys_from_ssh_keygen_read() {
    let info = inspect_private_key(KEYGEN_PRIVATE).unwrap();
    assert!(info.encrypted);
    assert_eq!(info.key_type, "ssh-ed25519");
    assert_eq!(info.fingerprint, KEYGEN_FINGERPRINT);
    let public = inspect_public_key(KEYGEN_PUBLIC).unwrap();
    assert_eq!(public.fingerprint, KEYGEN_FINGERPRINT);
    assert_eq!(public.comment, "test@example.com");
    assert_eq!(public.public_key, KEYGEN_PUBLIC);
    assert!(passphrase_opens(KEYGEN_PRIVATE, "correct horse").unwrap());
    assert!(!passphrase_opens(KEYGEN_PRIVATE, "wrong").unwrap());
    assert!(inspect_private_key("not a key").is_err());
}

#[test]
fn new_ed25519_keys() {
    let plain = generate_ed25519("me@example.com", None).unwrap();
    assert!(plain
        .private_key
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n"));
    assert!(plain
        .public_key
        .starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5"));
    assert!(plain.public_key.ends_with(" me@example.com"));
    assert_eq!(plain.key_type, "ssh-ed25519");
    assert!(plain.fingerprint.starts_with("SHA256:"));
    let info = inspect_private_key(&plain.private_key).unwrap();
    assert!(!info.encrypted);
    assert_eq!(info.public_key, plain.public_key);
    assert_eq!(info.fingerprint, plain.fingerprint);
    assert_eq!(
        inspect_public_key(&plain.public_key).unwrap().fingerprint,
        plain.fingerprint
    );

    let locked = generate_ed25519("me@example.com", Some("pass phrase")).unwrap();
    assert!(locked.public_key.ends_with(" me@example.com"));
    let info = inspect_private_key(&locked.private_key).unwrap();
    assert!(info.encrypted);
    assert_eq!(info.fingerprint, locked.fingerprint);
    assert!(passphrase_opens(&locked.private_key, "pass phrase").unwrap());
    assert_ne!(plain.fingerprint, locked.fingerprint);
}
