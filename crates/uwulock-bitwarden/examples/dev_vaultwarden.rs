//! A toy Vaultwarden for trying UwULock without a real server:
//!
//!   cargo run -p uwulock-bitwarden --example dev_vaultwarden
//!
//! Serves http://127.0.0.1:8087 (self-hosted, plain http is fine on this
//! computer). Account `nyu@uwu.local`, master password `uwu-nyu-nyu-nyu`,
//! with logins, a card, an identity, a note, an SSH key and an organisation.
//! `UWU_2FA=1` turns on two-step login: the email code is `123456`, and the
//! authenticator code is printed every 30 seconds. `UWU_TOY_ADDR` listens
//! elsewhere (the end-to-end run uses its own port).

#[path = "../tests/support/toy_server.rs"]
mod toy_server;

use toy_server::{Options, ToyServer};
use uwulock_bitwarden::totp::Totp;
use uwulock_bitwarden::Kdf;

fn main() {
    let two_factor = std::env::var("UWU_2FA").is_ok_and(|v| v == "1");
    let address = std::env::var("UWU_TOY_ADDR").unwrap_or_else(|_| "127.0.0.1:8087".into());
    let server = ToyServer::start_on(
        &address,
        Options {
            two_factor,
            kdf: Kdf::Pbkdf2 {
                iterations: 600_000,
            },
        },
    );
    println!("toy Vaultwarden on {}", server.url);
    println!("  email            {}", toy_server::EMAIL);
    println!("  master password  {}", toy_server::PASSWORD);
    if two_factor {
        println!(
            "  two-step login   email code {}, authenticator:",
            toy_server::EMAIL_CODE
        );
    }
    let totp = Totp::parse(toy_server::TOTP_SECRET).unwrap();
    loop {
        if two_factor {
            let (code, remaining) = totp.now();
            println!("    {} ({remaining} s)", code.as_str());
            std::thread::sleep(std::time::Duration::from_secs(remaining));
        } else {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}
