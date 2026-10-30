//! Links out of the app. The page names what to open; the host opens it
//! ([`Host::open_url`](crate::host::Host::open_url)).

use crate::App;

impl App {
    /// The project pages the app links to. The page names one; it never hands in
    /// an address of its own.
    pub fn open_project_page(&self, page: String) -> Result<(), String> {
        let url = match page.as_str() {
            "source" => "https://github.com/MinifyX/UwULock-Client",
            "releases" => "https://github.com/MinifyX/UwULock-Client/releases",
            "issues" => "https://github.com/MinifyX/UwULock-Client/issues",
            "license" => "https://www.gnu.org/licenses/gpl-3.0.html",
            "suite" => "https://uwu.minifyx.de",
            _ => return Err(format!("unknown page: {page}")),
        };
        self.open(url)
    }

    /// Opens a login's address in the browser. The page names the item and the
    /// address by position; only http and https leave the app.
    pub fn open_item_uri(&self, id: String, index: usize) -> Result<(), String> {
        let uri = self
            .vault()
            .item_uri(&id, index)
            .ok_or("This address can't be opened.")?;
        let parsed = url::Url::parse(&uri).map_err(|_| "This address can't be opened.")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Only web addresses open in the browser.".into());
        }
        self.open(parsed.as_str())
    }

    /// The account's web vault, for everything UwULock can't do yet.
    pub fn open_web_vault(&self) -> Result<(), String> {
        let url = self
            .vault()
            .web_vault()
            .ok_or("No account on this device.")?;
        self.open(&url)
    }

    fn open(&self, url: &str) -> Result<(), String> {
        self.host()
            .open_url(url)
            .map_err(|e| format!("Couldn't open the browser: {e}"))
    }
}
