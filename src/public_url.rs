use axum::http::HeaderMap;

pub fn infer_manager_base_url(headers: &HeaderMap, configured: Option<&str>) -> Option<String> {
    if let Some(value) = configured.and_then(normalize_base_url) {
        return Some(value);
    }

    let host =
        header_value(headers, "x-forwarded-host").or_else(|| header_value(headers, "host"))?;
    let proto = header_value(headers, "x-forwarded-proto").unwrap_or_else(|| "http".to_string());

    normalize_base_url(&format!("{proto}://{host}"))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_base_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn configured_url_wins() {
        let headers = HeaderMap::new();
        assert_eq!(
            infer_manager_base_url(&headers, Some("https://space.example.com/")),
            Some("https://space.example.com".to_string())
        );
    }

    #[test]
    fn infers_from_forwarded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("space.example.com"),
        );

        assert_eq!(
            infer_manager_base_url(&headers, None),
            Some("https://space.example.com".to_string())
        );
    }

    #[test]
    fn infers_from_host() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("192.168.1.168:20081"));

        assert_eq!(
            infer_manager_base_url(&headers, None),
            Some("http://192.168.1.168:20081".to_string())
        );
    }
}
