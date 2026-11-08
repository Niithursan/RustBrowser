use std::{error::Error, path::Path};

use http::Method;
use url::Url;

#[derive(Clone)]
pub struct Fixtures([Url; 2]);

impl Fixtures {
    pub fn new(run: &Path, pages: [&str; 2]) -> Result<Self, Box<dyn Error>> {
        let url = |page| Url::from_file_path(run.join(page)).map_err(|_| "Invalid fixture path");
        Ok(Self([url(pages[0])?, url(pages[1])?]))
    }

    pub fn page(&self, index: usize) -> &Url {
        &self.0[index]
    }

    pub fn allows(&self, request: &Url) -> bool {
        let mut location = request.clone();
        location.set_fragment(None);
        self.0.contains(&location)
    }

    pub fn allows_resource(&self, url: &Url, method: &Method, main_document: bool) -> bool {
        main_document && method == Method::GET && self.allows(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owned_fixture_locations_can_navigate() {
        let root = std::env::temp_dir().join("servo-policy-test");
        let fixtures = Fixtures::new(&root, ["first.html", "second.html"]).unwrap();
        assert!(fixtures.allows(fixtures.page(0)));
        assert!(fixtures.allows(fixtures.page(1)));
        assert!(fixtures.allows(&fixtures.page(0).join("#top").unwrap()));
        assert!(!fixtures.allows(&fixtures.page(0).join("?download=1").unwrap()));
        assert!(!fixtures.allows(&fixtures.page(0).join("../other.html").unwrap()));
        for target in [
            "https://example.invalid/",
            "http://127.0.0.1:8771/",
            "data:text/html,example",
            "javascript:void(0)",
            "mailto:fixture@example.invalid",
            "about:blank",
        ] {
            assert!(!fixtures.allows(&Url::parse(target).unwrap()), "{target}");
        }
    }

    #[test]
    fn resource_gate_allows_only_fixture_document_reads() {
        let root = std::env::temp_dir().join("servo-resource-policy-test");
        let fixtures = Fixtures::new(&root, ["first.html", "second.html"]).unwrap();
        let fixture = fixtures.page(0);
        assert!(fixtures.allows_resource(fixture, &Method::GET, true));
        assert!(!fixtures.allows_resource(fixture, &Method::GET, false));
        assert!(!fixtures.allows_resource(fixture, &Method::POST, true));
        let external = Url::parse("https://example.invalid/").unwrap();
        assert!(!fixtures.allows_resource(&external, &Method::GET, true));
    }
}
