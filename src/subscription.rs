//! Bounded subscription downloads. Fetching does not persist or apply a profile.

use crate::profile::{MAX_PROFILE_BYTES, Profile, ProfileError};
use std::{fmt, io::Read, time::Duration};
use url::Url;

#[derive(Debug)]
pub enum DownloadError {
    InvalidUrl,
    Request,
    TooLarge,
    InvalidText,
    InvalidProfile(ProfileError),
    NoProxies,
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("Enter an HTTPS subscription URL without user info"),
            Self::Request => f.write_str("Could not download the subscription"),
            Self::TooLarge => f.write_str("Subscription profile exceeds the size limit"),
            Self::InvalidText => f.write_str("Subscription did not return UTF-8 text"),
            Self::InvalidProfile(error) => error.fmt(f),
            Self::NoProxies => f.write_str("Subscription has no proxies or proxy providers"),
        }
    }
}

impl std::error::Error for DownloadError {}

/// Not Debug: the returned profile may include proxy credentials.
pub struct Download {
    pub yaml: String,
    pub suggested_interval_minutes: Option<u32>,
}

pub fn fetch(input: &str) -> Result<Download, DownloadError> {
    let url = parse_url(input)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(4)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    fetch_with(&agent, &url)
}

pub(crate) fn parse_url(input: &str) -> Result<Url, DownloadError> {
    let url = Url::parse(input.trim()).map_err(|_| DownloadError::InvalidUrl)?;
    if url.scheme() != "https"
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(DownloadError::InvalidUrl);
    }
    Ok(url)
}

fn fetch_with(agent: &ureq::Agent, url: &Url) -> Result<Download, DownloadError> {
    // Transport errors can include a secret URL or token, so do not keep or display them.
    let response = agent
        .get(url.as_str())
        .header(
            "User-Agent",
            concat!("Meow-Paw/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|_| DownloadError::Request)?;
    if response.status() != 200 {
        return Err(DownloadError::Request);
    }
    let suggested_interval_minutes = response
        .headers()
        .get("profile-update-interval")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|hours| *hours > 0)
        .and_then(|hours| hours.checked_mul(60));
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DownloadError::Request)?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(DownloadError::TooLarge);
    }
    let yaml = String::from_utf8(bytes).map_err(|_| DownloadError::InvalidText)?;
    let profile = Profile::parse(&yaml).map_err(DownloadError::InvalidProfile)?;
    if !profile.has_subscription_content() {
        return Err(DownloadError::NoProxies);
    }
    Ok(Download {
        yaml,
        suggested_interval_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};

    fn fixture(status: &str, headers: &str, body: &str) -> Result<Download, DownloadError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!(
            "http://{}/profile?secret=hidden",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            socket.write_all(response.as_bytes()).ok();
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .into();
        let result = fetch_with(&agent, &url);
        worker.join().unwrap();
        result
    }

    #[test]
    fn downloads_a_clash_profile_with_an_interval_hint() {
        let yaml = "proxies: [{name: Test, type: direct}]\n";
        let result = fixture("200 OK", "Profile-Update-Interval: 2\r\n", yaml).unwrap();
        assert_eq!(result.yaml, yaml);
        assert_eq!(result.suggested_interval_minutes, Some(120));
        let overflow = fixture("200 OK", "profile-update-interval: 4294967295\r\n", yaml).unwrap();
        assert_eq!(overflow.suggested_interval_minutes, None);
    }

    #[test]
    fn rejects_html_partial_downloads_and_unrelated_yamls() {
        assert!(matches!(
            fixture("200 OK", "", "<html>bad subscription</html>"),
            Err(DownloadError::InvalidProfile(_))
        ));
        assert!(matches!(
            fixture("206 Partial Content", "", "proxies: [{name: Test}]"),
            Err(DownloadError::Request)
        ));
        assert!(matches!(
            fixture("200 OK", "", "error: unavailable"),
            Err(DownloadError::NoProxies)
        ));
    }

    #[test]
    fn rejects_unencrypted_or_userinfo_urls_without_echoing_secrets() {
        for url in [
            "http://example.com/?token=password",
            "https://user:password@example.com",
            "ftp://example.com/?token=password",
            "https://example.com/#password",
        ] {
            assert!(matches!(parse_url(url), Err(DownloadError::InvalidUrl)));
        }
        assert!(parse_url(" https://example.com/?token=password ").is_ok());
    }
}
